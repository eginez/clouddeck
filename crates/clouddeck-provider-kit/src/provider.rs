use std::sync::Arc;

use clouddeck_provider_api::{
    Browse, Call, CellFailure, Connection, ConnectionId, DimValue, Dimension, DimensionId,
    ErrorKind, FieldDef, INSTANCES, InspectorSectionDef, Instance, KindId, KindInfo, ListResult,
    Provider, ProviderError, ProviderInfo, Selection, ValueId,
};

use crate::console_url::console_url_for_instance;
use crate::error::CloudDeckError;
use crate::fleet_compute::FleetComputeService;
use crate::service::{ComputeService, until_done};

#[cfg(test)]
use crate::models::{ComputeInstanceFilters, ComputeInstanceLoadResult};
#[cfg(test)]
use crate::service::BoxError;

type ServiceFactory = dyn Fn(&str, &str) -> Box<dyn ComputeService + Send + Sync> + Send + Sync;

const REGION: &str = "region";

pub struct ComputeServiceProvider {
    id: String,
    connections: Vec<Connection>,
    regions: Vec<String>,
    factory: Arc<ServiceFactory>,
    console: bool,
}

impl ComputeServiceProvider {
    pub fn new(
        id: impl Into<String>,
        connections: Vec<Connection>,
        regions: Vec<String>,
        factory: impl Fn(&str, &str) -> Box<dyn ComputeService + Send + Sync> + Send + Sync + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            connections,
            regions,
            factory: Arc::new(factory),
            console: false,
        }
    }

    #[cfg(test)]
    pub fn with_console_url(mut self, console: bool) -> Self {
        self.console = console;
        self
    }

    #[cfg(test)]
    pub fn from_shared(
        id: impl Into<String>,
        regions: Vec<String>,
        service: Arc<dyn ComputeService + Send + Sync>,
    ) -> Self {
        let connections = vec![Connection {
            id: "default".into(),
            display_name: "default".to_string(),
        }];
        Self::new(id, connections, regions, move |_, _| {
            Box::new(Forwarding(service.clone()))
        })
    }
}

#[cfg(test)]
struct Forwarding(Arc<dyn ComputeService + Send + Sync>);

#[cfg(test)]
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
            CloudDeckError::Auth => ProviderError::of(ErrorKind::Auth),
            CloudDeckError::Network => ProviderError::of(ErrorKind::Network),
            CloudDeckError::Timeout => ProviderError::of(ErrorKind::Timeout),
            CloudDeckError::Permission => ProviderError::of(ErrorKind::Permission),
            CloudDeckError::Throttled => ProviderError::of(ErrorKind::Throttled),
            CloudDeckError::NotFound => ProviderError::of(ErrorKind::NotFound),
            CloudDeckError::Unknown(message) => ProviderError::new(ErrorKind::Other, message),
        }
    }
}

fn region_cell(region: &str) -> Selection {
    Selection::new().with(REGION, vec![ValueId::from(region)])
}

fn normalized_regions(values: &[ValueId]) -> Vec<String> {
    let mut unique: Vec<String> = Vec::with_capacity(values.len());
    for value in values {
        let region = value.as_str().trim().to_string();
        if !region.is_empty() && !unique.contains(&region) {
            unique.push(region);
        }
    }
    unique
}

impl Provider for ComputeServiceProvider {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: self.id.clone(),
            name: if self.id == "aws" {
                "AWS".to_string()
            } else {
                self.id.clone()
            },
            version: env!("CARGO_PKG_VERSION").to_string(),
            connection_label: "Profile".to_string(),
        }
    }
}

impl Browse for ComputeServiceProvider {
    fn connections(&self, _cx: &Call<'_>) -> Result<Vec<Connection>, ProviderError> {
        Ok(self.connections.clone())
    }

    fn dimensions(
        &self,
        _cx: &Call<'_>,
        _connection: &ConnectionId,
    ) -> Result<Vec<Dimension>, ProviderError> {
        Ok(vec![Dimension {
            id: DimensionId::from(REGION),
            label: "Region".to_string(),
            plural_label: None,
            multi: true,
            optional: false,
            values: self
                .regions
                .iter()
                .filter(|region| region.as_str() != "fleet")
                .map(|region| DimValue {
                    id: ValueId::from(region.as_str()),
                    label: region.clone(),
                    parent: None,
                    default: false,
                })
                .collect(),
        }])
    }

    fn kinds(&self) -> Vec<KindInfo> {
        vec![KindInfo {
            id: KindId::instances(),
            label: "EC2 Instances".to_string(),
        }]
    }

    // Builds its own runtime, so callers must invoke it from a blocking thread, never inside a tokio runtime.
    fn list(
        &self,
        cx: &Call<'_>,
        connection: &ConnectionId,
        kind: &KindId,
        selection: &Selection,
    ) -> ListResult {
        if kind.as_str() != INSTANCES {
            return ListResult {
                resources: Vec::new(),
                failures: vec![CellFailure {
                    cell: selection.clone(),
                    error: ProviderError::unsupported(),
                }],
            };
        }

        let requested = normalized_regions(selection.values(REGION));
        if requested.is_empty() {
            return ListResult::default();
        }
        list_regions(&self.factory, cx, connection, &requested)
    }

    fn console_url(
        &self,
        _connection: &ConnectionId,
        kind: &KindId,
        resource: &Instance,
    ) -> Option<String> {
        if !self.console || kind.as_str() != INSTANCES {
            return None;
        }
        let instance_id = resource.id.rsplit(':').next().unwrap_or_default();
        console_url_for_instance(resource.region.as_deref().unwrap_or(""), instance_id).ok()
    }

    fn fields(&self, kind: &KindId) -> Vec<FieldDef> {
        if kind.as_str() == INSTANCES {
            crate::aws_fields()
        } else {
            Vec::new()
        }
    }

    fn inspector(&self, kind: &KindId) -> Vec<InspectorSectionDef> {
        if kind.as_str() == INSTANCES {
            crate::aws_inspector::aws_inspector()
        } else {
            Vec::new()
        }
    }
}

fn list_regions(
    factory: &Arc<ServiceFactory>,
    cx: &Call<'_>,
    connection: &ConnectionId,
    requested: &[String],
) -> ListResult {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            return all_regions_failed(
                requested,
                &CloudDeckError::Unknown(format!("could not start a runtime: {error}")),
            );
        }
    };

    let factory = factory.clone();
    let connection = connection.as_str().to_string();
    let fleet = FleetComputeService::new(requested.to_vec(), move |region: &str| {
        factory(&connection, region)
    });

    let outcome = match runtime.block_on(until_done(cx, fleet.list_instances(None))) {
        Ok(outcome) => outcome,
        Err(error) => return all_regions_failed(requested, &error),
    };
    match outcome {
        Ok(result) => ListResult {
            resources: result.instances.iter().map(Instance::from).collect(),
            failures: result
                .partial_failures
                .into_iter()
                .map(|failure| {
                    let mut error = ProviderError::from(failure.error);
                    error.message = failure.message;
                    CellFailure {
                        cell: region_cell(&failure.region),
                        error,
                    }
                })
                .collect(),
        },
        Err(error) => {
            let classified = error
                .downcast_ref::<CloudDeckError>()
                .cloned()
                .unwrap_or_else(|| CloudDeckError::from_service_error(error.as_ref()));
            all_regions_failed(requested, &classified)
        }
    }
}

fn all_regions_failed(regions: &[String], error: &CloudDeckError) -> ListResult {
    let message = error.user_message();
    ListResult {
        resources: Vec::new(),
        failures: regions
            .iter()
            .map(|region| CellFailure {
                cell: region_cell(region),
                error: ProviderError {
                    message: message.clone(),
                    ..ProviderError::from(error.clone())
                },
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

    struct NullHost;

    impl clouddeck_provider_api::Host for NullHost {
        fn log(&self, _level: clouddeck_provider_api::LogLevel, _message: &str) {}
        fn data_dir(&self) -> std::path::PathBuf {
            std::path::PathBuf::new()
        }
        fn setting(&self, _key: &str) -> Option<String> {
            None
        }
        fn secret(&self, _key: &str) -> Option<String> {
            None
        }
        fn set_secret(&self, _key: &str, _value: Option<&str>) -> Result<(), ProviderError> {
            Ok(())
        }
        fn open_url(&self, _url: &str) -> Result<(), ProviderError> {
            Ok(())
        }
        fn cancelled(&self) -> bool {
            false
        }
    }

    fn with_call<T>(f: impl FnOnce(&Call<'_>) -> T) -> T {
        let host = NullHost;
        f(&Call {
            deadline: None,
            host: &host,
        })
    }

    fn default_connection() -> ConnectionId {
        ConnectionId::from("default")
    }

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

    fn regions(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    fn selection(ids: &[&str]) -> Selection {
        Selection::new().with(REGION, ids.iter().map(|id| ValueId::from(*id)).collect())
    }

    fn list_regions(provider: &dyn Provider, ids: &[&str]) -> ListResult {
        with_call(|cx| {
            provider.list(
                cx,
                &default_connection(),
                &KindId::instances(),
                &selection(ids),
            )
        })
    }

    fn provider_with_failing_eu_west_1(console: bool) -> ComputeServiceProvider {
        ComputeServiceProvider::new(
            "test",
            vec![Connection {
                id: "default".into(),
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
    fn kinds_are_only_instances() {
        let provider = provider_with_failing_eu_west_1(false);
        assert_eq!(
            provider.kinds(),
            vec![KindInfo {
                id: KindId::instances(),
                label: "EC2 Instances".to_string(),
            }]
        );
    }

    #[test]
    fn dimensions_are_one_region_dimension_over_regions_without_fleet() {
        let expected: Vec<String> = crate::context::REGIONS
            .iter()
            .filter(|region| **region != "fleet")
            .map(|region| region.to_string())
            .collect();
        let provider =
            ComputeServiceProvider::new("aws", vec![], expected.clone(), |_, _| unreachable!());
        let dimensions =
            with_call(|cx| provider.dimensions(cx, &default_connection())).expect("dimensions");

        assert_eq!(dimensions.len(), 1);
        let region = &dimensions[0];
        assert_eq!(region.id, DimensionId::from(REGION));
        assert!(region.multi);
        assert!(!region.optional);
        let value_ids: Vec<&str> = region.values.iter().map(|v| v.id.as_str()).collect();
        let expected: Vec<&str> = expected.iter().map(String::as_str).collect();
        assert_eq!(value_ids, expected);
    }

    #[test]
    fn dimensions_drop_the_fleet_pseudo_region_from_the_raw_region_list() {
        let raw: Vec<String> = crate::context::REGIONS
            .iter()
            .map(|region| region.to_string())
            .collect();
        let provider = ComputeServiceProvider::new("aws", vec![], raw, |_, _| unreachable!());
        let dimensions =
            with_call(|cx| provider.dimensions(cx, &default_connection())).expect("dimensions");

        let value_ids: Vec<&str> = dimensions[0].values.iter().map(|v| v.id.as_str()).collect();
        let expected: Vec<&str> = crate::context::REGIONS
            .iter()
            .copied()
            .filter(|region| *region != "fleet")
            .collect();
        assert_eq!(value_ids, expected);
    }

    #[test]
    fn partial_failure_keeps_healthy_regions() {
        let provider = provider_with_failing_eu_west_1(false);
        let result = list_regions(&provider, &["us-east-1", "eu-west-1", "us-west-2"]);

        assert_eq!(result.resources.len(), 2);
        assert_eq!(result.resources[0].region.as_deref(), Some("us-east-1"));
        assert_eq!(result.resources[1].region.as_deref(), Some("us-west-2"));
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].cell, selection(&["eu-west-1"]));
        assert_eq!(result.failures[0].error.kind, ErrorKind::Network);
    }

    #[test]
    fn every_region_failing_reports_one_failure_per_region() {
        let provider = ComputeServiceProvider::new("test", vec![], vec![], |_, _| {
            Box::new(MockComputeService::new(
                vec![],
                Some(std::io::Error::other("boom").into()),
            ))
        });
        let result = list_regions(&provider, &["us-east-1", "us-west-2"]);

        assert!(result.resources.is_empty());
        assert_eq!(result.failures.len(), 2);
        assert_eq!(result.failures[0].cell, selection(&["us-east-1"]));
        assert_eq!(result.failures[1].cell, selection(&["us-west-2"]));
        assert_eq!(result.failures[1].error.kind, ErrorKind::Other);
        assert_eq!(result.failures[1].error.message, "boom");
    }

    #[test]
    fn empty_selection_returns_empty_result() {
        let provider = provider_with_failing_eu_west_1(false);
        assert_eq!(list_regions(&provider, &[]), ListResult::default());
    }

    #[test]
    fn unknown_kind_fails_the_selection_as_unsupported() {
        let provider = provider_with_failing_eu_west_1(true);
        let input = selection(&["us-east-1"]);
        let result = with_call(|cx| {
            provider.list(cx, &default_connection(), &KindId::from("services"), &input)
        });

        assert!(result.resources.is_empty());
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].cell, input);
        assert_eq!(result.failures[0].error.kind, ErrorKind::Unsupported);

        let kind = KindId::from("services");
        let instance = Instance::default();
        assert!(provider.fields(&kind).is_empty());
        assert!(provider.inspector(&kind).is_empty());
        assert_eq!(
            provider.console_url(&default_connection(), &kind, &instance),
            None
        );
    }

    #[test]
    fn cloud_deck_errors_map_one_to_one() {
        let kinds = [
            (CloudDeckError::Auth, ErrorKind::Auth),
            (CloudDeckError::Network, ErrorKind::Network),
            (CloudDeckError::Timeout, ErrorKind::Timeout),
            (CloudDeckError::Permission, ErrorKind::Permission),
            (CloudDeckError::Throttled, ErrorKind::Throttled),
            (CloudDeckError::NotFound, ErrorKind::NotFound),
        ];
        for (error, kind) in kinds {
            assert_eq!(ProviderError::from(error).kind, kind);
        }

        let unknown = ProviderError::from(CloudDeckError::Unknown("boom".to_string()));
        assert_eq!(unknown.kind, ErrorKind::Other);
        assert_eq!(unknown.message, "boom");
    }

    #[test]
    fn inspector_for_instances_is_the_aws_layout() {
        let mock = MockComputeService::new(vec![ec2("i-1")], None);
        let provider =
            ComputeServiceProvider::from_shared("mock", regions(&["us-east-1"]), Arc::new(mock));

        assert_eq!(
            provider.inspector(&KindId::instances()),
            crate::aws_inspector::aws_inspector()
        );
    }

    #[test]
    fn fields_for_instances_are_the_aws_registry() {
        let mock = MockComputeService::new(vec![ec2("i-1")], None);
        let provider =
            ComputeServiceProvider::from_shared("mock", regions(&["us-east-1"]), Arc::new(mock));
        let names: Vec<String> = provider
            .fields(&KindId::instances())
            .into_iter()
            .map(|f| f.name)
            .collect();
        let expected: Vec<String> = crate::aws_fields().into_iter().map(|f| f.name).collect();
        assert_eq!(names, expected);
    }

    #[test]
    fn from_shared_exposes_mock_and_fault_injecting_sources() {
        let mock = MockComputeService::new(vec![ec2("i-1")], None);
        let provider =
            ComputeServiceProvider::from_shared("mock", regions(&["us-east-1"]), Arc::new(mock));
        let result = list_regions(&provider, &["us-east-1"]);
        assert_eq!(result.resources.len(), 1);
        assert!(result.failures.is_empty());

        let faulted =
            FaultInjectingComputeService::new(MockComputeService::new(vec![ec2("i-2")], None));
        let provider = ComputeServiceProvider::from_shared(
            "faulted",
            regions(&["us-east-1"]),
            Arc::new(faulted),
        );
        let result = list_regions(&provider, &["us-east-1"]);
        assert_eq!(result.resources.len(), 1);
        assert_eq!(result.resources[0].id, "us-east-1:i-2");
    }

    #[test]
    fn console_url_matches_builder_when_enabled() {
        let instance = Instance::from(&EC2Instance {
            region: Some("us-east-1".to_string()),
            ..ec2("i-0123456789abcdef0")
        });
        let connection = default_connection();
        let instances = KindId::instances();

        let enabled = provider_with_failing_eu_west_1(true);
        assert_eq!(
            enabled.console_url(&connection, &instances, &instance),
            console_url_for_instance("us-east-1", "i-0123456789abcdef0").ok()
        );
        assert!(
            enabled
                .console_url(&connection, &instances, &instance)
                .is_some()
        );

        let disabled = provider_with_failing_eu_west_1(false);
        assert_eq!(
            disabled.console_url(&connection, &instances, &instance),
            None
        );
    }

    #[test]
    fn provider_is_usable_as_a_boxed_trait_object() {
        let provider: Box<dyn Provider> = Box::new(provider_with_failing_eu_west_1(false));
        let result = list_regions(provider.as_ref(), &["us-east-1"]);
        assert_eq!(result.resources.len(), 1);
    }
}
