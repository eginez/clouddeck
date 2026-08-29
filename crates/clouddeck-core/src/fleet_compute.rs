use crate::error::CloudDeckError;
use crate::models::{
    ComputeInstanceFilters, ComputeInstanceLoadResult, ComputeServicePartialFailure,
};
use crate::service::{BoxError, ComputeService};

/// Aggregates `list_instances` across multiple regions, each backed by a
/// per-region service produced by `service_factory` (Swift: `FleetComputeService`).
///
/// Per-region errors are collected as partial failures rather than aborting
/// the whole load; the aggregate only fails outright when every region
/// failed, matching the Swift oracle's "best effort across regions" behavior.
type ServiceFactory = dyn Fn(&str) -> Box<dyn ComputeService + Send + Sync> + Send + Sync;

pub struct FleetComputeService {
    regions: Vec<String>,
    service_factory: Box<ServiceFactory>,
}

impl FleetComputeService {
    /// `regions` are queried in order; `service_factory` builds the
    /// per-region `ComputeService` for a given region name.
    pub fn new(
        regions: Vec<String>,
        service_factory: impl Fn(&str) -> Box<dyn ComputeService + Send + Sync> + Send + Sync + 'static,
    ) -> Self {
        Self {
            regions,
            service_factory: Box::new(service_factory),
        }
    }
}

#[async_trait::async_trait]
impl ComputeService for FleetComputeService {
    async fn list_instances(
        &self,
        filters: Option<ComputeInstanceFilters>,
    ) -> Result<ComputeInstanceLoadResult, BoxError> {
        let mut aggregated_instances = Vec::new();
        let mut partial_failures: Vec<ComputeServicePartialFailure> = Vec::new();
        let mut had_successful_region_load = false;

        for region in &self.regions {
            let service = (self.service_factory)(region);
            match service.list_instances(filters.clone()).await {
                Ok(result) => {
                    had_successful_region_load = true;
                    aggregated_instances.extend(result.instances.into_iter().map(
                        |mut instance| {
                            instance.region = Some(region.clone());
                            instance
                        },
                    ));
                    partial_failures.extend(result.partial_failures);
                }
                Err(error) => {
                    let cloud_deck_error = CloudDeckError::from_service_error(error.as_ref());
                    partial_failures.push(ComputeServicePartialFailure {
                        region: region.clone(),
                        message: cloud_deck_error.user_message(),
                    });
                }
            }
        }

        aggregated_instances.sort_by(|a, b| {
            let region_a = a.region.as_deref().unwrap_or("");
            let region_b = b.region.as_deref().unwrap_or("");
            let region_cmp = region_a.to_lowercase().cmp(&region_b.to_lowercase());
            if region_cmp == std::cmp::Ordering::Equal {
                a.instance_id
                    .to_lowercase()
                    .cmp(&b.instance_id.to_lowercase())
            } else {
                region_cmp
            }
        });

        if !had_successful_region_load && let Some(first_failure) = partial_failures.first() {
            return Err(Box::new(CloudDeckError::Unknown(
                first_failure.message.clone(),
            )));
        }

        partial_failures.sort_by_key(|f| f.region.to_lowercase());

        Ok(ComputeInstanceLoadResult {
            instances: aggregated_instances,
            partial_failures,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockComputeService;
    use crate::models::EC2Instance;

    fn instance(instance_id: &str) -> EC2Instance {
        EC2Instance {
            instance_id: instance_id.to_string(),
            region: None,
            instance_type: None,
            state: Some("running".to_string()),
            launch_time: None,
            public_ip_address: None,
            private_ip_address: None,
            vpc_id: None,
            subnet_id: None,
            security_groups: vec![],
            tags: vec![],
        }
    }

    fn factory_for(
        regions_and_instances: Vec<(&'static str, Vec<EC2Instance>)>,
        failing_regions: Vec<&'static str>,
    ) -> impl Fn(&str) -> Box<dyn ComputeService + Send + Sync> + Send + Sync {
        move |region: &str| -> Box<dyn ComputeService + Send + Sync> {
            if failing_regions.contains(&region) {
                return Box::new(FailingService {
                    message: format!("{region} boom"),
                });
            }
            let instances = regions_and_instances
                .iter()
                .find(|(r, _)| *r == region)
                .map(|(_, instances)| instances.clone())
                .unwrap_or_default();
            Box::new(MockComputeService::new(instances, None))
        }
    }

    struct FailingService {
        message: String,
    }

    #[async_trait::async_trait]
    impl ComputeService for FailingService {
        async fn list_instances(
            &self,
            _filters: Option<ComputeInstanceFilters>,
        ) -> Result<ComputeInstanceLoadResult, BoxError> {
            Err(Box::new(CloudDeckError::Unknown(self.message.clone())))
        }
    }

    #[tokio::test]
    async fn aggregates_and_tags_instances_across_successful_regions() {
        let service = FleetComputeService::new(
            vec!["us-east-1".to_string(), "us-west-2".to_string()],
            factory_for(
                vec![
                    ("us-east-1", vec![instance("i-1")]),
                    ("us-west-2", vec![instance("i-2")]),
                ],
                vec![],
            ),
        );

        let result = service.list_instances(None).await.unwrap();
        assert!(result.partial_failures.is_empty());
        let tagged: Vec<(String, Option<String>)> = result
            .instances
            .iter()
            .map(|i| (i.instance_id.clone(), i.region.clone()))
            .collect();
        assert_eq!(
            tagged,
            vec![
                ("i-1".to_string(), Some("us-east-1".to_string())),
                ("i-2".to_string(), Some("us-west-2".to_string())),
            ]
        );
    }

    #[tokio::test]
    async fn partial_failures_collected_when_some_regions_fail() {
        let service = FleetComputeService::new(
            vec!["us-east-1".to_string(), "us-west-2".to_string()],
            factory_for(
                vec![("us-east-1", vec![instance("i-1")])],
                vec!["us-west-2"],
            ),
        );

        let result = service.list_instances(None).await.unwrap();
        assert_eq!(result.instances.len(), 1);
        assert_eq!(result.instances[0].instance_id, "i-1");
        assert_eq!(result.partial_failures.len(), 1);
        assert_eq!(result.partial_failures[0].region, "us-west-2");
        assert_eq!(result.partial_failures[0].message, "us-west-2 boom");
    }

    #[tokio::test]
    async fn throws_only_when_all_regions_fail() {
        let service = FleetComputeService::new(
            vec!["us-east-1".to_string(), "us-west-2".to_string()],
            factory_for(vec![], vec!["us-east-1", "us-west-2"]),
        );

        let result = service.list_instances(None).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "us-east-1 boom");
    }

    #[tokio::test]
    async fn instances_sorted_by_region_then_instance_id_case_insensitively() {
        let service = FleetComputeService::new(
            vec!["us-west-2".to_string(), "us-east-1".to_string()],
            factory_for(
                vec![
                    ("us-west-2", vec![instance("i-B"), instance("i-a")]),
                    ("us-east-1", vec![instance("i-z")]),
                ],
                vec![],
            ),
        );

        let result = service.list_instances(None).await.unwrap();
        let ids: Vec<&str> = result
            .instances
            .iter()
            .map(|i| i.instance_id.as_str())
            .collect();
        assert_eq!(ids, vec!["i-z", "i-a", "i-B"]);
    }

    #[tokio::test]
    async fn partial_failures_sorted_by_region_case_insensitively() {
        let service = FleetComputeService::new(
            vec!["Zeta".to_string(), "alpha".to_string()],
            factory_for(vec![], vec!["Zeta", "alpha"]),
        );

        // Both regions fail, so list_instances errors out with the first
        // failure encountered in region iteration order (not sorted order).
        let result = service.list_instances(None).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Zeta boom");
    }

    #[tokio::test]
    async fn partial_failures_sorted_when_mixed_with_success() {
        let service = FleetComputeService::new(
            vec!["Zeta".to_string(), "alpha".to_string(), "beta".to_string()],
            factory_for(vec![("beta", vec![instance("i-1")])], vec!["Zeta", "alpha"]),
        );

        let result = service.list_instances(None).await.unwrap();
        let regions: Vec<&str> = result
            .partial_failures
            .iter()
            .map(|f| f.region.as_str())
            .collect();
        assert_eq!(regions, vec!["alpha", "Zeta"]);
    }

    #[tokio::test]
    async fn empty_regions_yields_empty_result() {
        let service = FleetComputeService::new(vec![], factory_for(vec![], vec![]));
        let result = service.list_instances(None).await.unwrap();
        assert!(result.instances.is_empty());
        assert!(result.partial_failures.is_empty());
    }
}
