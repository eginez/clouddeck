use std::sync::Arc;

use clouddeck_provider_api::{
    Connection, FieldDef, Instance, ListResult, Provider, ProviderError, Scope, ScopeFailure,
};

use crate::aws_sdk::AwsSdkComputeService;
use crate::console_url::console_url_for_instance;
use crate::context::{REGIONS, load_profiles};
use crate::error::CloudDeckError;
use crate::fleet_compute::FleetComputeService;
use crate::models::{ComputeInstanceFilters, ComputeInstanceLoadResult};
use crate::service::{BoxError, ComputeService};

type ServiceFactory = dyn Fn(&str, &str) -> Box<dyn ComputeService + Send + Sync> + Send + Sync;

pub struct ComputeServiceProvider {
    id: String,
    connections: Vec<Connection>,
    scopes: Vec<Scope>,
    factory: Arc<ServiceFactory>,
    console: bool,
}

impl ComputeServiceProvider {
    pub fn new(
        id: impl Into<String>,
        connections: Vec<Connection>,
        scopes: Vec<Scope>,
        factory: impl Fn(&str, &str) -> Box<dyn ComputeService + Send + Sync> + Send + Sync + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            connections,
            scopes,
            factory: Arc::new(factory),
            console: false,
        }
    }

    pub fn with_console_url(mut self, console: bool) -> Self {
        self.console = console;
        self
    }

    pub fn from_shared(
        id: impl Into<String>,
        scopes: Vec<String>,
        service: Arc<dyn ComputeService + Send + Sync>,
    ) -> Self {
        let connections = vec![Connection {
            id: "default".to_string(),
            display_name: "default".to_string(),
        }];
        let scopes = scopes.into_iter().map(scope).collect();
        Self::new(id, connections, scopes, move |_, _| {
            Box::new(Forwarding(service.clone()))
        })
    }
}

pub fn aws_provider() -> ComputeServiceProvider {
    let connections = load_profiles()
        .unwrap_or_default()
        .into_iter()
        .map(|profile| Connection {
            id: profile.name.clone(),
            display_name: profile.name,
        })
        .collect();
    let scopes = REGIONS
        .iter()
        .filter(|region| **region != "fleet")
        .map(|region| scope(region.to_string()))
        .collect();
    ComputeServiceProvider::new("aws", connections, scopes, |profile, region| {
        Box::new(AwsSdkComputeService::new(profile, region))
    })
    .with_console_url(true)
}

fn scope(id: String) -> Scope {
    Scope {
        display_name: id.clone(),
        id,
    }
}

struct Forwarding(Arc<dyn ComputeService + Send + Sync>);

#[async_trait::async_trait]
impl ComputeService for Forwarding {
    async fn list_instances(
        &self,
        filters: Option<ComputeInstanceFilters>,
    ) -> Result<ComputeInstanceLoadResult, BoxError> {
        self.0.list_instances(filters).await
    }
}

impl From<CloudDeckError> for ProviderError {
    fn from(error: CloudDeckError) -> Self {
        match error {
            CloudDeckError::Auth => ProviderError::Auth,
            CloudDeckError::Network => ProviderError::Network,
            CloudDeckError::Timeout => ProviderError::Timeout,
            CloudDeckError::Permission => ProviderError::Permission,
            CloudDeckError::Throttled => ProviderError::Throttled,
            CloudDeckError::NotFound => ProviderError::NotFound,
            CloudDeckError::Unknown(message) => ProviderError::Other(message),
        }
    }
}

fn normalized_scopes(scopes: &[String]) -> Vec<String> {
    let mut unique: Vec<String> = Vec::with_capacity(scopes.len());
    for scope in scopes {
        let scope = scope.trim().to_string();
        if !scope.is_empty() && !unique.contains(&scope) {
            unique.push(scope);
        }
    }
    unique
}

impl Provider for ComputeServiceProvider {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn connections(&self) -> Result<Vec<Connection>, ProviderError> {
        Ok(self.connections.clone())
    }

    fn scopes(&self, _connection: &str) -> Result<Vec<Scope>, ProviderError> {
        Ok(self.scopes.clone())
    }

    // Builds its own runtime, so callers must invoke it from a blocking thread, never inside a tokio runtime.
    fn list(&self, connection: &str, scopes: &[String]) -> ListResult {
        let requested = normalized_scopes(scopes);
        if requested.is_empty() {
            return ListResult::default();
        }

        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                return all_scopes_failed(
                    &requested,
                    &CloudDeckError::Unknown(format!("could not start a runtime: {error}")),
                );
            }
        };

        let factory = self.factory.clone();
        let connection = connection.to_string();
        let fleet = FleetComputeService::new(requested.clone(), move |region: &str| {
            factory(&connection, region)
        });

        match runtime.block_on(fleet.list_instances(None)) {
            Ok(result) => ListResult {
                instances: result.instances.iter().map(Instance::from).collect(),
                failures: result
                    .partial_failures
                    .into_iter()
                    .map(|failure| ScopeFailure {
                        scope: failure.region,
                        message: failure.message,
                        error: failure.error.into(),
                    })
                    .collect(),
            },
            Err(error) => {
                let classified = error
                    .downcast_ref::<CloudDeckError>()
                    .cloned()
                    .unwrap_or_else(|| CloudDeckError::from_service_error(error.as_ref()));
                all_scopes_failed(&requested, &classified)
            }
        }
    }

    fn console_url(&self, instance: &Instance) -> Option<String> {
        if !self.console {
            return None;
        }
        let instance_id = instance.id.rsplit(':').next().unwrap_or_default();
        console_url_for_instance(instance.region.as_deref().unwrap_or(""), instance_id).ok()
    }

    fn fields(&self) -> Vec<FieldDef> {
        crate::aws_fields()
    }
}

fn all_scopes_failed(scopes: &[String], error: &CloudDeckError) -> ListResult {
    let message = error.user_message();
    ListResult {
        instances: Vec::new(),
        failures: scopes
            .iter()
            .map(|scope| ScopeFailure {
                scope: scope.clone(),
                message: message.clone(),
                error: error.clone().into(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fault_injecting::FaultInjectingComputeService;
    use crate::mock::MockComputeService;
    use crate::models::EC2Instance;

    fn ec2(instance_id: &str) -> EC2Instance {
        EC2Instance {
            instance_id: instance_id.to_string(),
            region: None,
            instance_type: Some("t3.micro".to_string()),
            state: Some("running".to_string()),
            launch_time: None,
            public_ip_address: None,
            private_ip_address: None,
            vpc_id: None,
            subnet_id: None,
            security_groups: vec![],
            tags: vec![],
            availability_zone: None,
            image_id: None,
            key_name: None,
            architecture: None,
            state_transition_reason: None,
            iam_instance_profile_arn: None,
            private_dns_name: None,
            public_dns_name: None,
        }
    }

    fn scopes(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    fn provider_with_failing_eu_west_1(console: bool) -> ComputeServiceProvider {
        ComputeServiceProvider::new(
            "test",
            vec![Connection {
                id: "default".to_string(),
                display_name: "default".to_string(),
            }],
            vec![],
            |_, region| {
                if region == "eu-west-1" {
                    Box::new(MockComputeService::new(
                        vec![],
                        Some(std::io::Error::other("network connection lost").into()),
                    ))
                } else {
                    Box::new(MockComputeService::new(
                        vec![ec2(&format!("i-{region}"))],
                        None,
                    ))
                }
            },
        )
        .with_console_url(console)
    }

    #[test]
    fn partial_failure_keeps_healthy_scopes() {
        let provider = provider_with_failing_eu_west_1(false);
        let result = provider.list("default", &scopes(&["us-east-1", "eu-west-1", "us-west-2"]));

        assert_eq!(result.instances.len(), 2);
        assert_eq!(result.instances[0].region.as_deref(), Some("us-east-1"));
        assert_eq!(result.instances[1].region.as_deref(), Some("us-west-2"));
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].scope, "eu-west-1");
        assert_eq!(result.failures[0].error, ProviderError::Network);
    }

    #[test]
    fn every_scope_failing_reports_one_failure_per_scope() {
        let provider = ComputeServiceProvider::new("test", vec![], vec![], |_, _| {
            Box::new(MockComputeService::new(
                vec![],
                Some(std::io::Error::other("boom").into()),
            ))
        });
        let result = provider.list("default", &scopes(&["us-east-1", "us-west-2"]));

        assert!(result.instances.is_empty());
        assert_eq!(result.failures.len(), 2);
        assert_eq!(result.failures[0].scope, "us-east-1");
        assert_eq!(result.failures[1].scope, "us-west-2");
        assert_eq!(
            result.failures[1].error,
            ProviderError::Other("boom".to_string())
        );
    }

    #[test]
    fn empty_scopes_return_empty_result() {
        let provider = provider_with_failing_eu_west_1(false);
        assert_eq!(provider.list("default", &[]), ListResult::default());
    }

    #[test]
    fn cloud_deck_errors_map_one_to_one() {
        assert_eq!(
            ProviderError::from(CloudDeckError::Auth),
            ProviderError::Auth
        );
        assert_eq!(
            ProviderError::from(CloudDeckError::Network),
            ProviderError::Network
        );
        assert_eq!(
            ProviderError::from(CloudDeckError::Timeout),
            ProviderError::Timeout
        );
        assert_eq!(
            ProviderError::from(CloudDeckError::Permission),
            ProviderError::Permission
        );
        assert_eq!(
            ProviderError::from(CloudDeckError::Throttled),
            ProviderError::Throttled
        );
        assert_eq!(
            ProviderError::from(CloudDeckError::NotFound),
            ProviderError::NotFound
        );
        assert_eq!(
            ProviderError::from(CloudDeckError::Unknown("boom".to_string())),
            ProviderError::Other("boom".to_string())
        );
    }

    #[test]
    fn fields_are_the_aws_registry() {
        let mock = MockComputeService::new(vec![ec2("i-1")], None);
        let provider =
            ComputeServiceProvider::from_shared("mock", scopes(&["us-east-1"]), Arc::new(mock));
        let names: Vec<String> = provider.fields().into_iter().map(|f| f.name).collect();
        let expected: Vec<String> = crate::aws_fields().into_iter().map(|f| f.name).collect();
        assert_eq!(names, expected);
    }

    #[test]
    fn from_shared_exposes_mock_and_fault_injecting_sources() {
        let mock = MockComputeService::new(vec![ec2("i-1")], None);
        let provider =
            ComputeServiceProvider::from_shared("mock", scopes(&["us-east-1"]), Arc::new(mock));
        let result = provider.list("default", &scopes(&["us-east-1"]));
        assert_eq!(result.instances.len(), 1);
        assert!(result.failures.is_empty());

        let faulted =
            FaultInjectingComputeService::new(MockComputeService::new(vec![ec2("i-2")], None));
        let provider = ComputeServiceProvider::from_shared(
            "faulted",
            scopes(&["us-east-1"]),
            Arc::new(faulted),
        );
        let result = provider.list("default", &scopes(&["us-east-1"]));
        assert_eq!(result.instances.len(), 1);
        assert_eq!(result.instances[0].id, "us-east-1:i-2");
    }

    #[test]
    fn console_url_matches_builder_when_enabled() {
        let instance = Instance::from(&EC2Instance {
            region: Some("us-east-1".to_string()),
            ..ec2("i-0123456789abcdef0")
        });

        let enabled = provider_with_failing_eu_west_1(true);
        assert_eq!(
            enabled.console_url(&instance),
            console_url_for_instance("us-east-1", "i-0123456789abcdef0").ok()
        );
        assert!(enabled.console_url(&instance).is_some());

        let disabled = provider_with_failing_eu_west_1(false);
        assert_eq!(disabled.console_url(&instance), None);
    }

    #[test]
    fn provider_is_usable_as_a_boxed_trait_object() {
        let provider: Box<dyn Provider> = Box::new(provider_with_failing_eu_west_1(false));
        let result = provider.list("default", &scopes(&["us-east-1"]));
        assert_eq!(result.instances.len(), 1);
    }
}
