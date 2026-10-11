use crate::error::CloudDeckError;
use crate::models::{
    ComputeInstanceFilters, ComputeInstanceLoadResult, ComputeServicePartialFailure,
};
use crate::service::{BoxError, ComputeService};
use std::sync::Arc;
use std::time::Duration;

/// Aggregates `list_instances` across multiple regions, each backed by a
/// per-region service produced by `service_factory` (Swift: `FleetComputeService`).
///
/// Per-region errors are collected as partial failures rather than aborting
/// the whole load; the aggregate only fails outright when every region
/// failed, matching the Swift oracle's "best effort across regions" behavior.
type ServiceFactory = dyn Fn(&str) -> Box<dyn ComputeService + Send + Sync> + Send + Sync;

const DEFAULT_REGION_TIMEOUT: Duration = Duration::from_secs(65);

pub struct FleetComputeService {
    regions: Vec<String>,
    service_factory: Arc<ServiceFactory>,
    region_timeout: Duration,
}

impl FleetComputeService {
    /// `regions` are queried in order; `service_factory` builds the
    /// per-region `ComputeService` for a given region name.
    pub fn new(
        regions: Vec<String>,
        service_factory: impl Fn(&str) -> Box<dyn ComputeService + Send + Sync> + Send + Sync + 'static,
    ) -> Self {
        Self::with_timeout(regions, service_factory, DEFAULT_REGION_TIMEOUT)
    }

    /// Builds a fleet service with a per-region deadline.
    ///
    /// Region requests run concurrently, so a fleet refresh is bounded by one
    /// region deadline rather than the sum of every region deadline. The
    /// shorter constructor is useful for deterministic tests and callers that
    /// have a tighter UI budget.
    pub fn with_timeout(
        regions: Vec<String>,
        service_factory: impl Fn(&str) -> Box<dyn ComputeService + Send + Sync> + Send + Sync + 'static,
        region_timeout: Duration,
    ) -> Self {
        let mut unique_regions = Vec::with_capacity(regions.len());
        for region in regions {
            let region = region.trim().to_string();
            if !region.is_empty() && !unique_regions.iter().any(|seen| seen == &region) {
                unique_regions.push(region);
            }
        }
        Self {
            regions: unique_regions,
            service_factory: Arc::new(service_factory),
            region_timeout,
        }
    }
}

#[async_trait::async_trait]
impl ComputeService for FleetComputeService {
    async fn list_instances(
        &self,
        filters: Option<ComputeInstanceFilters>,
    ) -> Result<ComputeInstanceLoadResult, BoxError> {
        let mut tasks = tokio::task::JoinSet::new();
        for (index, region) in self.regions.iter().cloned().enumerate() {
            let service_factory = self.service_factory.clone();
            let filters = filters.clone();
            let region_timeout = self.region_timeout;
            tasks.spawn(async move {
                let service = (service_factory)(&region);
                let result =
                    tokio::time::timeout(region_timeout, service.list_instances(filters)).await;
                let result = match result {
                    Ok(result) => result,
                    Err(_) => Err(std::io::Error::other(format!(
                        "AWS region load timed out after {} seconds",
                        region_timeout.as_secs_f64()
                    ))
                    .into()),
                };
                (index, region, result)
            });
        }

        let mut completed = Vec::with_capacity(self.regions.len());
        while let Some(joined) = tasks.join_next().await {
            match joined {
                Ok(item) => completed.push(item),
                Err(error) => {
                    // A panic in one region must not strand the remaining
                    // regions or turn a useful partial result into a process
                    // panic. This is only reachable for a panicking service
                    // factory or task runtime failure, so no region can be
                    // recovered from the JoinError itself.
                    completed.push((
                        usize::MAX,
                        "<unknown>".to_string(),
                        Err(
                            std::io::Error::other(format!("AWS region load task failed: {error}"))
                                .into(),
                        ),
                    ));
                }
            }
        }

        // Completion order is intentionally independent of result order. Use
        // the original region order for deterministic errors and diagnostics.
        completed.sort_by_key(|(index, _, _)| *index);
        let mut aggregated_instances = Vec::new();
        let mut partial_failures: Vec<ComputeServicePartialFailure> = Vec::new();
        let mut had_successful_region_load = false;

        for (_, region, result) in completed {
            match result {
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
                    let cloud_deck_error = error
                        .downcast_ref::<CloudDeckError>()
                        .cloned()
                        .unwrap_or_else(|| CloudDeckError::from_service_error(error.as_ref()));
                    partial_failures.push(ComputeServicePartialFailure {
                        region: region.clone(),
                        message: cloud_deck_error.user_message(),
                        error: cloud_deck_error,
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
            return Err(Box::new(first_failure.error.clone()));
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
    use std::sync::Arc;

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

    struct DelayedService {
        delay: Duration,
        instance_id: String,
        barrier: Option<Arc<tokio::sync::Barrier>>,
    }

    #[async_trait::async_trait]
    impl ComputeService for DelayedService {
        async fn list_instances(
            &self,
            _filters: Option<ComputeInstanceFilters>,
        ) -> Result<ComputeInstanceLoadResult, BoxError> {
            if let Some(barrier) = &self.barrier {
                barrier.wait().await;
            }
            tokio::time::sleep(self.delay).await;
            Ok(ComputeInstanceLoadResult {
                instances: vec![instance(&self.instance_id)],
                partial_failures: vec![],
            })
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

    struct ThrottledService;

    #[async_trait::async_trait]
    impl ComputeService for ThrottledService {
        async fn list_instances(
            &self,
            _filters: Option<ComputeInstanceFilters>,
        ) -> Result<ComputeInstanceLoadResult, BoxError> {
            Err(Box::new(CloudDeckError::Throttled))
        }
    }

    #[tokio::test]
    async fn partial_failure_keeps_classified_error() {
        let service = FleetComputeService::new(
            vec!["us-east-1".to_string(), "us-west-2".to_string()],
            |region: &str| -> Box<dyn ComputeService + Send + Sync> {
                if region == "us-west-2" {
                    Box::new(ThrottledService)
                } else {
                    Box::new(MockComputeService::new(vec![instance("i-1")], None))
                }
            },
        );

        let result = service.list_instances(None).await.unwrap();
        assert_eq!(result.partial_failures.len(), 1);
        assert_eq!(result.partial_failures[0].error, CloudDeckError::Throttled);
        assert_eq!(
            result.partial_failures[0].message,
            CloudDeckError::Throttled.user_message()
        );
    }

    #[tokio::test]
    async fn all_regions_throttled_surfaces_typed_throttled_error() {
        let service = FleetComputeService::new(
            vec!["us-east-1".to_string(), "us-west-2".to_string()],
            |_: &str| -> Box<dyn ComputeService + Send + Sync> { Box::new(ThrottledService) },
        );

        let error = service.list_instances(None).await.unwrap_err();
        assert_eq!(
            error.downcast_ref::<CloudDeckError>(),
            Some(&CloudDeckError::Throttled)
        );
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

    #[tokio::test]
    async fn region_loads_run_concurrently_with_one_deadline() {
        let service = FleetComputeService::with_timeout(
            vec!["one".into(), "two".into(), "three".into()],
            {
                let barrier = Arc::new(tokio::sync::Barrier::new(3));
                move |region| {
                    Box::new(DelayedService {
                        delay: Duration::from_millis(1),
                        instance_id: format!("i-{region}"),
                        barrier: Some(barrier.clone()),
                    })
                }
            },
            Duration::from_millis(200),
        );

        let result = tokio::time::timeout(Duration::from_secs(1), service.list_instances(None))
            .await
            .expect("all region tasks should start together")
            .unwrap();

        assert_eq!(result.instances.len(), 3);
    }

    #[tokio::test]
    async fn slow_region_becomes_partial_failure_at_deadline() {
        let service = FleetComputeService::with_timeout(
            vec!["fast".into(), "slow".into()],
            |region| {
                Box::new(DelayedService {
                    delay: if region == "slow" {
                        Duration::from_millis(100)
                    } else {
                        Duration::from_millis(1)
                    },
                    instance_id: format!("i-{region}"),
                    barrier: None,
                })
            },
            Duration::from_millis(20),
        );

        let result = service.list_instances(None).await.unwrap();
        assert_eq!(result.instances.len(), 1);
        assert_eq!(result.instances[0].instance_id, "i-fast");
        assert_eq!(result.partial_failures.len(), 1);
        assert_eq!(result.partial_failures[0].region, "slow");
        assert!(result.partial_failures[0].message.contains("timed out"));
    }

    #[tokio::test]
    async fn duplicate_and_blank_regions_are_queried_once() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let captured = seen.clone();
        let service = FleetComputeService::new(
            vec![
                "us-east-1".into(),
                " ".into(),
                "us-east-1".into(),
                "us-west-2".into(),
            ],
            move |region| {
                captured.lock().unwrap().push(region.to_string());
                Box::new(MockComputeService::new(vec![], None))
            },
        );

        service.list_instances(None).await.unwrap();
        let mut seen = seen.lock().unwrap().clone();
        seen.sort();
        assert_eq!(seen, vec!["us-east-1".to_string(), "us-west-2".to_string()]);
    }
}
