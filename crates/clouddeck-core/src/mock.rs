use std::error::Error as StdError;
use std::sync::Arc;

use crate::models::{ComputeInstanceFilters, ComputeInstanceLoadResult, EC2Instance};
use crate::service::{BoxError, ComputeService};

/// In-memory `ComputeService` (Swift: `MockComputeService`).
///
/// Returns a fixed instance list (filtered, sorted by instance ID) or a fixed
/// error, configured at construction.
///
/// `BoxError` is not `Clone`, so the configured error lives behind an `Arc`
/// and is re-boxed on every call — matching the Swift oracle, whose `Error`
/// values are `Copy` references.
#[derive(Debug, Default, Clone)]
pub struct MockComputeService {
    instances: Vec<EC2Instance>,
    list_instances_error: Option<Arc<dyn StdError + Send + Sync>>,
}

impl MockComputeService {
    pub fn new(instances: Vec<EC2Instance>, list_instances_error: Option<BoxError>) -> Self {
        Self {
            instances,
            list_instances_error: list_instances_error.map(Arc::from),
        }
    }

    pub fn instances(&self) -> &[EC2Instance] {
        &self.instances
    }

    pub fn list_instances_error(&self) -> Option<BoxError> {
        self.list_instances_error
            .as_ref()
            .map(|error| Box::new(error.clone()) as BoxError)
    }

    /// Sorts by instance ID ascending with a case-insensitive comparison
    /// (Swift: `localizedCaseInsensitiveCompare`).
    fn sort_by_instance_id(instances: &mut [EC2Instance]) {
        instances.sort_by(|a, b| {
            a.instance_id
                .to_lowercase()
                .cmp(&b.instance_id.to_lowercase())
                .then_with(|| a.instance_id.cmp(&b.instance_id))
        });
    }
}

#[async_trait::async_trait]
impl ComputeService for MockComputeService {
    async fn list_instances(
        &self,
        filters: Option<ComputeInstanceFilters>,
    ) -> Result<ComputeInstanceLoadResult, BoxError> {
        if let Some(error) = &self.list_instances_error {
            return Err(Box::new(error.clone()));
        }

        let mut filtered_instances = match &filters {
            Some(filters) => filters.apply(&self.instances),
            None => self.instances.clone(),
        };
        Self::sort_by_instance_id(&mut filtered_instances);

        Ok(ComputeInstanceLoadResult {
            instances: filtered_instances,
            partial_failures: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::EC2InstanceTag;

    #[derive(Debug)]
    struct TestError(&'static str);

    impl std::fmt::Display for TestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.0)
        }
    }

    impl StdError for TestError {}

    fn boxed_error(message: &'static str) -> BoxError {
        Box::new(TestError(message))
    }

    fn instance(instance_id: &str, state: &str, name: &str) -> EC2Instance {
        EC2Instance {
            instance_id: instance_id.to_string(),
            region: None,
            instance_type: None,
            state: Some(state.to_string()),
            launch_time: None,
            public_ip_address: None,
            private_ip_address: None,
            vpc_id: None,
            subnet_id: None,
            security_groups: vec![],
            tags: vec![EC2InstanceTag {
                key: "Name".to_string(),
                value: name.to_string(),
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

    #[tokio::test]
    async fn returns_configured_instances_sorted_by_instance_id_case_insensitively() {
        let service = MockComputeService::new(
            vec![
                instance("i-Zeta", "running", "z"),
                instance("i-alpha", "running", "a"),
                instance("i-Beta", "running", "b"),
            ],
            None,
        );

        let result = service.list_instances(None).await.unwrap();
        let ids: Vec<&str> = result
            .instances
            .iter()
            .map(|i| i.instance_id.as_str())
            .collect();
        assert_eq!(ids, vec!["i-alpha", "i-Beta", "i-Zeta"]);
        assert!(result.partial_failures.is_empty());
    }

    #[tokio::test]
    async fn applies_filters_before_sorting() {
        let service = MockComputeService::new(
            vec![
                instance("i-batch", "stopped", "batch"),
                instance("i-api", "running", "api"),
                instance("i-queue", "running", "queue"),
            ],
            None,
        );

        let filters = ComputeInstanceFilters {
            name_contains: None,
            states: vec!["running".to_string()],
            instance_types: vec![],
        };
        let result = service.list_instances(Some(filters)).await.unwrap();
        let ids: Vec<&str> = result
            .instances
            .iter()
            .map(|i| i.instance_id.as_str())
            .collect();
        assert_eq!(ids, vec!["i-api", "i-queue"]);
    }

    #[tokio::test]
    async fn returns_empty_result_when_no_instances_configured() {
        let service = MockComputeService::default();

        let result = service.list_instances(None).await.unwrap();
        assert!(result.instances.is_empty());
    }

    #[tokio::test]
    async fn returns_configured_error_when_set() {
        let service = MockComputeService::new(
            vec![instance("i-1", "running", "x")],
            Some(boxed_error("injected failure")),
        );

        let result = service.list_instances(None).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "injected failure");
    }

    #[tokio::test]
    async fn error_takes_precedence_over_instances() {
        let service = MockComputeService::new(
            vec![instance("i-1", "running", "x")],
            Some(boxed_error("boom")),
        );

        let filters = ComputeInstanceFilters {
            name_contains: Some("no-match".to_string()),
            states: vec![],
            instance_types: vec![],
        };
        let result = service.list_instances(Some(filters)).await;
        assert!(result.is_err());
    }
}
