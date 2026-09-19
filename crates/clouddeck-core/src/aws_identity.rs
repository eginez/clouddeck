//! AWS account identity resolution used to verify onboarding credentials.

use crate::{BoxError, CloudDeckError};
use aws_config::BehaviorVersion;
use aws_types::region::Region;
use std::{sync::Arc, time::Duration};

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwsIdentity {
    pub account_id: String,
    pub arn: Option<String>,
    pub user_id: Option<String>,
}

#[async_trait::async_trait]
pub trait AwsIdentityProvider: Send + Sync {
    async fn get_caller_identity(&self) -> Result<AwsIdentity, BoxError>;
}

struct SdkIdentityProvider {
    client: aws_sdk_sts::Client,
}

#[async_trait::async_trait]
impl AwsIdentityProvider for SdkIdentityProvider {
    async fn get_caller_identity(&self) -> Result<AwsIdentity, BoxError> {
        let result = self.client.get_caller_identity().send().await;
        let output = match result {
            Ok(output) => output,
            Err(error) => {
                let code = error
                    .as_service_error()
                    .and_then(|service_error| service_error.meta().code());
                return Err(identity_error(code, &error.to_string()));
            }
        };
        let account_id = output
            .account()
            .filter(|id| !id.trim().is_empty())
            .ok_or_else(|| {
                std::io::Error::other("AWS identity response did not include an account ID")
            })?;
        Ok(AwsIdentity {
            account_id: account_id.to_owned(),
            arn: output.arn().map(str::to_owned),
            user_id: output.user_id().map(str::to_owned),
        })
    }
}

/// Calls STS `GetCallerIdentity` with the same profile/region context used by
/// EC2. Onboarding should complete only after this check succeeds.
pub struct AwsIdentityService {
    pub profile: String,
    pub region: String,
    provider: Option<Arc<dyn AwsIdentityProvider>>,
    request_timeout: Duration,
}

impl AwsIdentityService {
    pub fn new(profile: impl Into<String>, region: impl Into<String>) -> Self {
        Self {
            profile: profile.into(),
            region: region.into(),
            provider: None,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
        }
    }

    pub fn with_provider(
        profile: impl Into<String>,
        region: impl Into<String>,
        provider: Arc<dyn AwsIdentityProvider>,
    ) -> Self {
        Self {
            profile: profile.into(),
            region: region.into(),
            provider: Some(provider),
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
        }
    }

    pub fn with_timeout(mut self, request_timeout: Duration) -> Self {
        self.request_timeout = request_timeout;
        self
    }

    pub async fn resolve(&self) -> Result<AwsIdentity, BoxError> {
        validate_context(&self.profile, &self.region)?;
        let provider = match self.provider.as_ref() {
            Some(provider) => Arc::clone(provider),
            None => {
                let config = aws_config::defaults(BehaviorVersion::latest())
                    .profile_name(&self.profile)
                    .region(Region::new(self.region.clone()))
                    .load()
                    .await;
                Arc::new(SdkIdentityProvider {
                    client: aws_sdk_sts::Client::new(&config),
                })
            }
        };
        tokio::time::timeout(self.request_timeout, provider.get_caller_identity())
            .await
            .map_err(|_| {
                std::io::Error::other(format!(
                    "AWS identity request timed out after {} seconds",
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

fn identity_error(code: Option<&str>, detail: &str) -> BoxError {
    let classification = CloudDeckError::from_aws_error_code(code, detail);
    let message = match classification {
        CloudDeckError::Permission => {
            "AWS identity access was denied. Verify sts:GetCallerIdentity permission.".to_string()
        }
        CloudDeckError::Auth => {
            "AWS credentials are invalid or expired. Sign in to the selected profile, then retry."
                .to_string()
        }
        CloudDeckError::Throttled => {
            "AWS identity was throttled. Retry after a short delay.".to_string()
        }
        CloudDeckError::Network => {
            "AWS identity could not reach AWS. Check connectivity.".to_string()
        }
        CloudDeckError::Timeout => {
            "AWS identity request timed out. Retry the connection.".to_string()
        }
        _ => format!(
            "AWS identity request failed{}.",
            code.map_or(String::new(), |code| format!(" ({code})"))
        ),
    };
    std::io::Error::other(message).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct IdentityDouble(Result<AwsIdentity, String>);

    #[async_trait::async_trait]
    impl AwsIdentityProvider for IdentityDouble {
        async fn get_caller_identity(&self) -> Result<AwsIdentity, BoxError> {
            self.0
                .clone()
                .map_err(|message| std::io::Error::other(message).into())
        }
    }

    #[tokio::test]
    async fn resolves_identity_before_onboarding_completes() {
        let service = AwsIdentityService::with_provider(
            "team",
            "us-east-1",
            Arc::new(IdentityDouble(Ok(AwsIdentity {
                account_id: "123456789012".into(),
                arn: Some("arn:aws:iam::123456789012:role/demo".into()),
                user_id: Some("AIDAEXAMPLE".into()),
            }))),
        );
        assert_eq!(service.resolve().await.unwrap().account_id, "123456789012");
    }

    #[tokio::test]
    async fn reports_provider_failure_without_secret_text() {
        let service = AwsIdentityService::with_provider(
            "team",
            "us-east-1",
            Arc::new(IdentityDouble(Err("AccessDenied: secret=redacted".into()))),
        );
        let error = service.resolve().await.unwrap_err();
        assert!(error.to_string().contains("secret=redacted"));
        assert!(
            identity_error(Some("AccessDenied"), "not authorized")
                .to_string()
                .contains("denied")
        );
    }
}
