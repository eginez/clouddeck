use std::future::Future;
use std::time::Duration;

use clouddeck_provider_api::Call;

use crate::error::CloudDeckError;
use crate::models::{ComputeInstanceFilters, ComputeInstanceLoadResult};

pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

const CANCEL_POLL: Duration = Duration::from_millis(50);

/// Ends `work` early when the call's deadline passes or the host cancels the call.
pub(crate) async fn until_done<T>(
    cx: &Call<'_>,
    work: impl Future<Output = T>,
) -> Result<T, CloudDeckError> {
    tokio::pin!(work);
    let deadline = cx.deadline.map(tokio::time::Instant::from_std);
    let mut cancel_poll = tokio::time::interval(CANCEL_POLL);
    loop {
        tokio::select! {
            output = &mut work => return Ok(output),
            () = sleep_until(deadline) => return Err(CloudDeckError::Timeout),
            _ = cancel_poll.tick() => {
                if cx.host.cancelled() {
                    return Err(CloudDeckError::Unknown("The load was cancelled.".to_string()));
                }
            }
        }
    }
}

async fn sleep_until(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

/// Cloud-agnostic compute fleet seam (Swift: `ComputeServiceProtocol`).
///
/// `BoxError` rather than `CloudDeckError` because callers must be able to
/// inject and downcast arbitrary errors unrelated to `CloudDeckError` (see
/// fault-injection use cases in the Swift oracle's test suite).
#[async_trait::async_trait]
pub trait ComputeService {
    async fn list_instances(
        &self,
        filters: Option<ComputeInstanceFilters>,
    ) -> Result<ComputeInstanceLoadResult, BoxError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::EC2Instance;
    use clouddeck_provider_api::Host;
    use std::time::Instant;

    struct TestDoubleComputeService {
        instances: Vec<EC2Instance>,
    }

    #[async_trait::async_trait]
    impl ComputeService for TestDoubleComputeService {
        async fn list_instances(
            &self,
            filters: Option<ComputeInstanceFilters>,
        ) -> Result<ComputeInstanceLoadResult, BoxError> {
            let instances = match filters {
                Some(filters) => filters.apply(&self.instances),
                None => self.instances.clone(),
            };
            Ok(ComputeInstanceLoadResult {
                instances,
                partial_failures: vec![],
            })
        }
    }

    #[tokio::test]
    async fn usable_as_a_boxed_trait_object() {
        let service: Box<dyn ComputeService + Send + Sync> = Box::new(TestDoubleComputeService {
            instances: vec![EC2Instance {
                instance_id: "i-1".to_string(),
                region: None,
                instance_type: None,
                state: None,
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
            }],
        });

        let result = service.list_instances(None).await.unwrap();
        assert_eq!(result.instances.len(), 1);
    }

    struct CancelledHost(bool);

    impl Host for CancelledHost {
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

        fn set_secret(
            &self,
            _key: &str,
            _value: Option<&str>,
        ) -> Result<(), clouddeck_provider_api::ProviderError> {
            Ok(())
        }

        fn open_url(&self, _url: &str) -> Result<(), clouddeck_provider_api::ProviderError> {
            Ok(())
        }

        fn cancelled(&self) -> bool {
            self.0
        }
    }

    #[tokio::test]
    async fn until_done_ends_a_stuck_call_once_the_host_cancels() {
        let host = CancelledHost(true);
        let cx = Call {
            deadline: None,
            host: &host,
        };
        let outcome = tokio::time::timeout(
            Duration::from_secs(2),
            until_done(&cx, std::future::pending::<()>()),
        )
        .await
        .expect("until_done must return promptly after cancellation");
        assert!(matches!(outcome, Err(CloudDeckError::Unknown(_))));
    }

    #[tokio::test]
    async fn until_done_times_out_a_stuck_call_past_its_deadline() {
        let host = CancelledHost(false);
        let cx = Call {
            deadline: Some(Instant::now()),
            host: &host,
        };
        let outcome = tokio::time::timeout(
            Duration::from_secs(2),
            until_done(&cx, std::future::pending::<()>()),
        )
        .await
        .expect("until_done must return promptly at the deadline");
        assert!(matches!(outcome, Err(CloudDeckError::Timeout)));
    }
}
