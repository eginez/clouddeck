//! AWS SDK backed EC2 access.
//!
//! The SDK owns credential resolution (including cached IAM Identity Center
//! credentials).  CloudDeck only supplies the selected profile and region;
//! it never shells out to the AWS CLI.

use crate::{
    BoxError, CloudDeckError, ComputeInstanceFilters, ComputeInstanceLoadResult, ComputeService,
    EC2Instance, EC2InstanceTag, EC2SecurityGroup,
};
use aws_config::BehaviorVersion;
use aws_types::region::Region;
use std::{collections::HashSet, sync::Arc, time::Duration};

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// One page returned by [`Ec2PageSource`].  Keeping pagination behind this
/// small seam makes failure and continuation behavior testable without AWS.
#[derive(Debug, Clone, PartialEq)]
pub struct Ec2Page {
    pub instances: Vec<EC2Instance>,
    pub next_token: Option<String>,
}

#[async_trait::async_trait]
pub trait Ec2PageSource: Send + Sync {
    async fn next_page(&self, next_token: Option<String>) -> Result<Ec2Page, BoxError>;
}

struct SdkEc2PageSource {
    client: aws_sdk_ec2::Client,
    region: String,
}

#[async_trait::async_trait]
impl Ec2PageSource for SdkEc2PageSource {
    async fn next_page(&self, next_token: Option<String>) -> Result<Ec2Page, BoxError> {
        let result = self
            .client
            .describe_instances()
            .set_next_token(next_token)
            .send()
            .await;

        let output = match result {
            Ok(output) => output,
            Err(error) => {
                let code = error
                    .as_service_error()
                    .and_then(|service_error| service_error.meta().code());
                return Err(aws_request_error(
                    "EC2 DescribeInstances",
                    code,
                    &error.to_string(),
                ));
            }
        };

        let instances = output
            .reservations()
            .iter()
            .flat_map(|reservation| reservation.instances())
            .filter_map(|instance| map_instance(instance, &self.region))
            .collect();

        Ok(Ec2Page {
            instances,
            next_token: output.next_token().map(str::to_owned),
        })
    }
}

/// EC2 service using the Rust AWS SDK and the SDK's standard provider chain.
pub struct AwsSdkComputeService {
    pub profile: String,
    pub region: String,
    page_source: Option<Arc<dyn Ec2PageSource>>,
    request_timeout: Duration,
}

impl AwsSdkComputeService {
    pub fn new(profile: impl Into<String>, region: impl Into<String>) -> Self {
        Self {
            profile: profile.into(),
            region: region.into(),
            page_source: None,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
        }
    }

    /// Builds a service around a page source for fast, deterministic tests or
    /// an embedding application's own transport boundary.
    pub fn with_page_source(
        profile: impl Into<String>,
        region: impl Into<String>,
        page_source: Arc<dyn Ec2PageSource>,
    ) -> Self {
        Self {
            profile: profile.into(),
            region: region.into(),
            page_source: Some(page_source),
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
        }
    }

    pub fn with_timeout(mut self, request_timeout: Duration) -> Self {
        self.request_timeout = request_timeout;
        self
    }

    async fn page_source(&self) -> Result<Arc<dyn Ec2PageSource>, BoxError> {
        if let Some(source) = self.page_source.as_ref() {
            return Ok(Arc::clone(source));
        }

        validate_context(&self.profile, &self.region)?;
        let config = aws_config::defaults(BehaviorVersion::latest())
            .profile_name(&self.profile)
            .region(Region::new(self.region.clone()))
            .load()
            .await;
        Ok(Arc::new(SdkEc2PageSource {
            client: aws_sdk_ec2::Client::new(&config),
            region: self.region.clone(),
        }))
    }

    async fn load(
        &self,
        filters: Option<ComputeInstanceFilters>,
    ) -> Result<ComputeInstanceLoadResult, BoxError> {
        validate_context(&self.profile, &self.region)?;
        let source = self.page_source().await?;
        let mut instances = Vec::new();
        let mut next_token = None;
        let mut seen_tokens = HashSet::new();

        loop {
            let page = source.next_page(next_token.clone()).await?;
            instances.extend(page.instances);
            let Some(token) = page.next_token.filter(|token| !token.trim().is_empty()) else {
                break;
            };
            if !seen_tokens.insert(token.clone()) {
                return Err(std::io::Error::other(
                    "AWS EC2 pagination returned a repeated continuation token",
                )
                .into());
            }
            next_token = Some(token);
        }

        let instances =
            filters.map_or_else(|| instances.clone(), |filters| filters.apply(&instances));
        Ok(ComputeInstanceLoadResult {
            instances,
            partial_failures: vec![],
        })
    }
}

#[async_trait::async_trait]
impl ComputeService for AwsSdkComputeService {
    async fn list_instances(
        &self,
        filters: Option<ComputeInstanceFilters>,
    ) -> Result<ComputeInstanceLoadResult, BoxError> {
        tokio::time::timeout(self.request_timeout, self.load(filters))
            .await
            .map_err(|_| {
                std::io::Error::other(format!(
                    "AWS EC2 request timed out after {} seconds",
                    self.request_timeout.as_secs_f64()
                ))
            })?
    }
}

fn validate_context(profile: &str, region: &str) -> std::io::Result<()> {
    if profile.trim().is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "AWS profile cannot be empty",
        ));
    }
    if region.trim().is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "AWS region cannot be empty",
        ));
    }
    Ok(())
}

fn aws_request_error(operation: &str, code: Option<&str>, detail: &str) -> BoxError {
    let classification = CloudDeckError::from_aws_error_code(code, detail);
    let message = match classification {
        CloudDeckError::Permission => {
            format!(
                "{operation} was denied. Verify the selected profile has ec2:DescribeInstances permission."
            )
        }
        CloudDeckError::Auth => {
            format!(
                "{operation} could not authenticate. Sign in to the selected AWS profile, then retry."
            )
        }
        CloudDeckError::Throttled => {
            format!("{operation} was throttled. Retry after a short delay.")
        }
        CloudDeckError::Timeout => format!("{operation} timed out. Retry the request."),
        CloudDeckError::Network => format!("{operation} could not reach AWS. Check connectivity."),
        CloudDeckError::NotFound => format!("{operation} could not find the requested resource."),
        CloudDeckError::Unknown(_) => format!(
            "{operation} failed{}.",
            code.map_or(String::new(), |code| format!(" ({code})"))
        ),
    };
    std::io::Error::other(message).into()
}

fn map_instance(instance: &aws_sdk_ec2::types::Instance, region: &str) -> Option<EC2Instance> {
    let instance_id = instance.instance_id()?.to_owned();
    if instance_id.trim().is_empty() {
        return None;
    }
    Some(EC2Instance {
        instance_id,
        region: Some(region.to_owned()),
        instance_type: instance
            .instance_type()
            .map(|value| value.as_str().to_owned()),
        state: instance
            .state()
            .and_then(|state| state.name())
            .map(|value| value.as_str().to_owned()),
        launch_time: instance
            .launch_time()
            .and_then(|value| std::time::SystemTime::try_from(value.clone()).ok()),
        public_ip_address: instance.public_ip_address().map(str::to_owned),
        private_ip_address: instance.private_ip_address().map(str::to_owned),
        vpc_id: instance.vpc_id().map(str::to_owned),
        subnet_id: instance.subnet_id().map(str::to_owned),
        security_groups: instance
            .security_groups()
            .iter()
            .map(|group| EC2SecurityGroup {
                group_id: group.group_id().map(str::to_owned),
                group_name: group.group_name().map(str::to_owned),
            })
            .collect(),
        tags: instance
            .tags()
            .iter()
            .filter_map(|tag| {
                Some(EC2InstanceTag {
                    key: tag.key()?.to_owned(),
                    value: tag.value()?.to_owned(),
                })
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Pages {
        pages: Mutex<Vec<Result<Ec2Page, String>>>,
    }

    #[async_trait::async_trait]
    impl Ec2PageSource for Pages {
        async fn next_page(&self, _next_token: Option<String>) -> Result<Ec2Page, BoxError> {
            self.pages
                .lock()
                .unwrap()
                .remove(0)
                .map_err(|message| std::io::Error::other(message).into())
        }
    }

    fn instance(id: &str, name: &str) -> EC2Instance {
        EC2Instance {
            instance_id: id.into(),
            region: Some("us-east-1".into()),
            instance_type: Some("t3.micro".into()),
            state: Some("running".into()),
            launch_time: None,
            public_ip_address: None,
            private_ip_address: None,
            vpc_id: None,
            subnet_id: None,
            security_groups: vec![],
            tags: vec![EC2InstanceTag {
                key: "Name".into(),
                value: name.into(),
            }],
        }
    }

    #[tokio::test]
    async fn follows_continuation_tokens_and_applies_filters() {
        let pages = Arc::new(Pages {
            pages: Mutex::new(vec![
                Ok(Ec2Page {
                    instances: vec![instance("i-a", "api")],
                    next_token: Some("page-2".into()),
                }),
                Ok(Ec2Page {
                    instances: vec![instance("i-b", "worker")],
                    next_token: None,
                }),
            ]),
        });
        let service = AwsSdkComputeService::with_page_source("team", "us-east-1", pages);
        let result = service
            .list_instances(Some(ComputeInstanceFilters {
                name_contains: Some("api".into()),
                ..Default::default()
            }))
            .await
            .unwrap();
        assert_eq!(
            result
                .instances
                .iter()
                .map(|i| i.instance_id.as_str())
                .collect::<Vec<_>>(),
            ["i-a"]
        );
    }

    #[tokio::test]
    async fn rejects_repeated_continuation_tokens() {
        let pages = Arc::new(Pages {
            pages: Mutex::new(vec![
                Ok(Ec2Page {
                    instances: vec![],
                    next_token: Some("same".into()),
                }),
                Ok(Ec2Page {
                    instances: vec![],
                    next_token: Some("same".into()),
                }),
            ]),
        });
        let service = AwsSdkComputeService::with_page_source("team", "us-east-1", pages);
        let error = service.list_instances(None).await.unwrap_err();
        assert!(error.to_string().contains("repeated continuation"));
    }

    #[tokio::test]
    async fn preserves_page_source_failures_without_logging_credentials() {
        let pages = Arc::new(Pages {
            pages: Mutex::new(vec![Err("AccessDenied: not authorized".into())]),
        });
        let service = AwsSdkComputeService::with_page_source("team", "us-east-1", pages);
        let error = service.list_instances(None).await.unwrap_err();
        assert!(error.to_string().contains("AccessDenied"));
        assert!(!error.to_string().contains("secret"));
    }
}
