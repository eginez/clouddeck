//! Pure rules for keeping a failed region's previous rows, marked stale, after a partial refresh.

use std::collections::HashSet;

use crate::models::{ComputeInstanceLoadResult, EC2Instance};

/// Refreshed instances plus carried-over rows from regions that failed to load.
#[derive(Debug, Clone, PartialEq)]
pub struct MergedLoad {
    /// Fresh instances followed by previous instances from failed regions.
    pub instances: Vec<EC2Instance>,
    /// Failed regions that have previous rows, shown as stale.
    pub stale_regions: Vec<String>,
}

/// Merges a refresh result with the previous rows, keeping rows from regions that failed.
pub fn merge_with_previous(
    previous: &[EC2Instance],
    result: &ComputeInstanceLoadResult,
) -> MergedLoad {
    let failed: Vec<&str> = result
        .partial_failures
        .iter()
        .map(|failure| failure.region.as_str())
        .collect();
    let answered: HashSet<String> = result.instances.iter().map(EC2Instance::id).collect();

    let carried = previous.iter().filter(|instance| {
        instance
            .region
            .as_deref()
            .is_some_and(|region| failed.contains(&region))
            && !answered.contains(&instance.id())
    });
    let mut instances = result.instances.clone();
    instances.extend(carried.cloned());

    let mut stale_regions: Vec<String> = Vec::new();
    for region in &failed {
        let has_previous = previous
            .iter()
            .any(|instance| instance.region.as_deref() == Some(*region));
        if has_previous && !stale_regions.iter().any(|stale| stale == region) {
            stale_regions.push(region.to_string());
        }
    }

    MergedLoad {
        instances,
        stale_regions,
    }
}

/// Whether the instance belongs to a region whose rows are stale.
pub fn is_stale_instance(instance: &EC2Instance, stale_regions: &[String]) -> bool {
    instance
        .region
        .as_deref()
        .is_some_and(|region| stale_regions.iter().any(|stale| stale == region))
}

/// Number of instances in the given region.
pub fn stale_count(instances: &[EC2Instance], region: &str) -> usize {
    instances
        .iter()
        .filter(|instance| instance.region.as_deref() == Some(region))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ComputeServicePartialFailure;

    fn instance(region: &str, instance_id: &str) -> EC2Instance {
        EC2Instance {
            instance_id: instance_id.to_string(),
            region: Some(region.to_string()),
            instance_type: None,
            state: None,
            launch_time: None,
            public_ip_address: None,
            private_ip_address: None,
            vpc_id: None,
            subnet_id: None,
            security_groups: Vec::new(),
            tags: Vec::new(),
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

    fn failure(region: &str) -> ComputeServicePartialFailure {
        ComputeServicePartialFailure {
            region: region.to_string(),
            message: "request failed".to_string(),
        }
    }

    fn result(instances: Vec<EC2Instance>, failures: &[&str]) -> ComputeInstanceLoadResult {
        ComputeInstanceLoadResult {
            instances,
            partial_failures: failures.iter().map(|region| failure(region)).collect(),
        }
    }

    #[test]
    fn failed_region_keeps_previous_rows_and_is_marked_stale() {
        let previous = vec![
            instance("us-east-1", "a"),
            instance("eu-central-1", "b"),
            instance("eu-central-1", "c"),
        ];
        let merged = merge_with_previous(
            &previous,
            &result(vec![instance("us-east-1", "a2")], &["eu-central-1"]),
        );
        assert_eq!(
            merged,
            MergedLoad {
                instances: vec![
                    instance("us-east-1", "a2"),
                    instance("eu-central-1", "b"),
                    instance("eu-central-1", "c"),
                ],
                stale_regions: vec!["eu-central-1".to_string()],
            }
        );
    }

    #[test]
    fn answered_region_is_not_carried_over() {
        let previous = vec![
            instance("us-east-1", "a"),
            instance("eu-central-1", "b"),
            instance("eu-central-1", "c"),
        ];
        let merged = merge_with_previous(
            &previous,
            &result(
                vec![instance("us-east-1", "a2"), instance("eu-central-1", "b")],
                &[],
            ),
        );
        assert_eq!(
            merged,
            MergedLoad {
                instances: vec![instance("us-east-1", "a2"), instance("eu-central-1", "b")],
                stale_regions: Vec::new(),
            }
        );
    }

    #[test]
    fn failed_region_without_previous_rows_is_not_stale() {
        let previous = vec![instance("us-east-1", "a")];
        let merged = merge_with_previous(
            &previous,
            &result(vec![instance("us-east-1", "a")], &["eu-west-1"]),
        );
        assert_eq!(
            merged,
            MergedLoad {
                instances: vec![instance("us-east-1", "a")],
                stale_regions: Vec::new(),
            }
        );
    }

    #[test]
    fn fresh_instance_is_not_duplicated_by_previous_row() {
        let previous = vec![instance("eu-central-1", "b")];
        let merged = merge_with_previous(
            &previous,
            &result(vec![instance("eu-central-1", "b")], &["eu-central-1"]),
        );
        assert_eq!(
            merged,
            MergedLoad {
                instances: vec![instance("eu-central-1", "b")],
                stale_regions: vec!["eu-central-1".to_string()],
            }
        );
    }

    #[test]
    fn duplicate_failures_list_region_once() {
        let previous = vec![instance("eu-central-1", "b")];
        let merged = merge_with_previous(
            &previous,
            &result(Vec::new(), &["eu-central-1", "eu-central-1"]),
        );
        assert_eq!(
            merged,
            MergedLoad {
                instances: vec![instance("eu-central-1", "b")],
                stale_regions: vec!["eu-central-1".to_string()],
            }
        );
    }

    #[test]
    fn no_previous_rows_returns_result_instances() {
        let fresh = vec![instance("us-east-1", "a")];
        let merged = merge_with_previous(&[], &result(fresh.clone(), &["eu-central-1"]));
        assert_eq!(merged.instances, fresh);
        assert!(merged.stale_regions.is_empty());
    }

    #[test]
    fn stale_instance_matches_its_region() {
        let stale_regions = vec!["eu-central-1".to_string()];
        assert!(is_stale_instance(
            &instance("eu-central-1", "b"),
            &stale_regions
        ));
        assert!(!is_stale_instance(
            &instance("us-east-1", "a"),
            &stale_regions
        ));
    }

    #[test]
    fn stale_count_counts_instances_in_region() {
        let instances = vec![
            instance("us-east-1", "a"),
            instance("eu-central-1", "b"),
            instance("eu-central-1", "c"),
        ];
        assert_eq!(stale_count(&instances, "eu-central-1"), 2);
    }
}
