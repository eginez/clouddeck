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
use aws_sdk_ec2::operation::describe_instances::DescribeInstancesError;
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
            Err(error) => return Err(sdk_request_error("EC2 DescribeInstances", &error)),
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

fn sdk_request_error<R: std::fmt::Debug + 'static>(
    operation: &str,
    error: &aws_sdk_ec2::error::SdkError<DescribeInstancesError, R>,
) -> BoxError {
    let code = error
        .as_service_error()
        .and_then(|service_error| service_error.meta().code());
    aws_request_error(operation, code, &sdk_error_detail(error))
}

/// Transport and credential SDK errors display only their outer layer ("dispatch
/// failure", "failed to construct request"); the real reason is in the source chain.
/// Service errors keep their own message so unrelated text cannot match a keyword.
pub(crate) fn sdk_error_detail<E, R>(error: &aws_sdk_ec2::error::SdkError<E, R>) -> String
where
    E: std::error::Error + 'static,
    R: std::fmt::Debug + 'static,
{
    if error.as_service_error().is_some() {
        error.to_string()
    } else {
        error_chain_text(error)
    }
}

fn error_chain_text(error: &(dyn std::error::Error + 'static)) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

fn aws_request_error(operation: &str, code: Option<&str>, detail: &str) -> BoxError {
    match CloudDeckError::from_aws_error_code(code, detail) {
        CloudDeckError::Unknown(_) => std::io::Error::other(format!(
            "{operation} failed{}.",
            code.map_or(String::new(), |code| format!(" ({code})"))
        ))
        .into(),
        classified => Box::new(classified),
    }
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
            .and_then(|value| std::time::SystemTime::try_from(*value).ok()),
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
        availability_zone: instance
            .placement()
            .and_then(|p| p.availability_zone())
            .map(str::to_owned),
        image_id: instance.image_id().map(str::to_owned),
        key_name: instance.key_name().map(str::to_owned),
        architecture: instance.architecture().map(|value| value.as_str().to_owned()),
        state_transition_reason: instance
            .state_transition_reason()
            .map(str::to_owned)
            .filter(|value| !value.is_empty()),
        iam_instance_profile_arn: instance
            .iam_instance_profile()
            .and_then(|profile| profile.arn())
            .map(str::to_owned),
        private_dns_name: instance.private_dns_name().map(str::to_owned),
        public_dns_name: instance.public_dns_name().map(str::to_owned),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Pages {
        pages: Mutex<Vec<Result<Ec2Page, BoxError>>>,
        requested_tokens: Mutex<Vec<Option<String>>>,
    }

    fn scripted_pages(pages: Vec<Result<Ec2Page, BoxError>>) -> Arc<Pages> {
        Arc::new(Pages {
            pages: Mutex::new(pages),
            requested_tokens: Mutex::new(Vec::new()),
        })
    }

    fn page(instances: Vec<EC2Instance>, next_token: Option<&str>) -> Ec2Page {
        Ec2Page {
            instances,
            next_token: next_token.map(str::to_owned),
        }
    }

    #[async_trait::async_trait]
    impl Ec2PageSource for Pages {
        async fn next_page(&self, next_token: Option<String>) -> Result<Ec2Page, BoxError> {
            self.requested_tokens.lock().unwrap().push(next_token);
            self.pages.lock().unwrap().remove(0)
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

    #[test]
    fn request_errors_keep_their_typed_classification() {
        let not_found = aws_request_error("EC2 DescribeInstances", Some("InstanceNotFound"), "");
        assert_eq!(
            not_found.downcast_ref::<CloudDeckError>(),
            Some(&CloudDeckError::NotFound)
        );

        let network = aws_request_error(
            "EC2 DescribeInstances",
            None,
            "dispatch failure: connection refused",
        );
        assert_eq!(
            network.downcast_ref::<CloudDeckError>(),
            Some(&CloudDeckError::Network)
        );
    }

    #[test]
    fn expired_sso_token_in_sdk_source_chain_is_auth() {
        #[derive(Debug)]
        struct CredentialsFailure(std::io::Error);

        impl std::fmt::Display for CredentialsFailure {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("failed to load credentials")
            }
        }

        impl std::error::Error for CredentialsFailure {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }

        let sdk_error =
            aws_sdk_ec2::error::SdkError::<DescribeInstancesError>::construction_failure(Box::new(
                CredentialsFailure(std::io::Error::other(
                    "the SSO token has expired and cannot be refreshed",
                )),
            ));
        assert_eq!(sdk_error.to_string(), "failed to construct request");

        let mapped = sdk_request_error("EC2 DescribeInstances", &sdk_error);
        assert_eq!(
            mapped.downcast_ref::<CloudDeckError>(),
            Some(&CloudDeckError::Auth)
        );
    }

    #[test]
    fn service_error_text_does_not_widen_classification() {
        let sdk_error = aws_sdk_ec2::error::SdkError::<DescribeInstancesError, ()>::service_error(
            DescribeInstancesError::generic(
                aws_sdk_ec2::error::ErrorMetadata::builder()
                    .code("InvalidNextToken")
                    .message("The token provided is invalid")
                    .build(),
            ),
            (),
        );

        let mapped = sdk_request_error("EC2 DescribeInstances", &sdk_error);
        assert_eq!(mapped.downcast_ref::<CloudDeckError>(), None);
        assert_eq!(
            mapped.to_string(),
            "EC2 DescribeInstances failed (InvalidNextToken)."
        );
    }

    #[tokio::test]
    async fn follows_continuation_tokens_and_applies_filters() {
        let pages = scripted_pages(vec![
            Ok(page(vec![instance("i-a", "api")], Some("page-2"))),
            Ok(page(vec![instance("i-b", "worker")], None)),
        ]);
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
        let pages = scripted_pages(vec![
            Ok(page(vec![], Some("same"))),
            Ok(page(vec![], Some("same"))),
        ]);
        let service = AwsSdkComputeService::with_page_source("team", "us-east-1", pages);
        let error = service.list_instances(None).await.unwrap_err();
        assert!(error.to_string().contains("repeated continuation"));
    }

    #[tokio::test]
    async fn preserves_page_source_failures_without_logging_credentials() {
        let pages = scripted_pages(vec![Err("AccessDenied: not authorized".into())]);
        let service = AwsSdkComputeService::with_page_source("team", "us-east-1", pages);
        let error = service.list_instances(None).await.unwrap_err();
        assert!(error.to_string().contains("AccessDenied"));
        assert!(!error.to_string().contains("secret"));
    }

    #[tokio::test]
    async fn three_pages_load_every_instance_and_request_each_token_in_order() {
        let pages = scripted_pages(vec![
            Ok(page(vec![instance("i-1", "a")], Some("page-2"))),
            Ok(page(vec![instance("i-2", "b")], Some("page-3"))),
            Ok(page(vec![instance("i-3", "c")], None)),
        ]);
        let service = AwsSdkComputeService::with_page_source("team", "us-east-1", pages.clone());

        let result = service.list_instances(None).await.unwrap();

        let ids: Vec<&str> = result
            .instances
            .iter()
            .map(|i| i.instance_id.as_str())
            .collect();
        assert_eq!(ids, ["i-1", "i-2", "i-3"]);
        assert!(result.partial_failures.is_empty());
        assert_eq!(
            *pages.requested_tokens.lock().unwrap(),
            vec![None, Some("page-2".to_string()), Some("page-3".to_string())]
        );
    }

    #[tokio::test]
    async fn region_with_no_instances_yields_empty_result() {
        let pages = scripted_pages(vec![Ok(page(vec![], None))]);
        let service = AwsSdkComputeService::with_page_source("team", "eu-west-1", pages);

        let result = service.list_instances(None).await.unwrap();

        assert!(result.instances.is_empty());
        assert!(result.partial_failures.is_empty());
    }

    #[tokio::test]
    async fn failing_region_is_recorded_while_other_regions_keep_their_instances() {
        let fleet = crate::FleetComputeService::new(
            vec!["eu-west-1".into(), "us-west-2".into()],
            |region: &str| -> Box<dyn ComputeService + Send + Sync> {
                let pages = if region == "us-west-2" {
                    scripted_pages(vec![Err(Box::new(CloudDeckError::Permission))])
                } else {
                    scripted_pages(vec![Ok(page(vec![instance("i-eu", "api")], None))])
                };
                Box::new(AwsSdkComputeService::with_page_source(
                    "team", region, pages,
                ))
            },
        );

        let result = fleet.list_instances(None).await.unwrap();

        assert_eq!(result.instances.len(), 1);
        assert_eq!(result.instances[0].instance_id, "i-eu");
        assert_eq!(result.instances[0].region.as_deref(), Some("eu-west-1"));
        assert_eq!(result.partial_failures.len(), 1);
        assert_eq!(result.partial_failures[0].region, "us-west-2");
        assert_eq!(result.partial_failures[0].error, CloudDeckError::Permission);
        assert_eq!(
            result.partial_failures[0].message,
            CloudDeckError::Permission.user_message()
        );
    }

    #[tokio::test]
    async fn throttling_mid_pagination_fails_the_region_with_typed_error() {
        let pages = scripted_pages(vec![
            Ok(page(vec![instance("i-1", "a")], Some("page-2"))),
            Err(aws_request_error(
                "EC2 DescribeInstances",
                Some("RequestLimitExceeded"),
                "",
            )),
        ]);
        let service = AwsSdkComputeService::with_page_source("team", "us-east-1", pages.clone());

        let error = service.list_instances(None).await.unwrap_err();

        assert_eq!(
            error.downcast_ref::<CloudDeckError>(),
            Some(&CloudDeckError::Throttled)
        );
        assert_eq!(pages.requested_tokens.lock().unwrap().len(), 2);
    }
}
