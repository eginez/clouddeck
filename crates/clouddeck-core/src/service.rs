use crate::models::{ComputeInstanceFilters, ComputeInstanceLoadResult};

pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

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
            }],
        });

        let result = service.list_instances(None).await.unwrap();
        assert_eq!(result.instances.len(), 1);
    }
}
