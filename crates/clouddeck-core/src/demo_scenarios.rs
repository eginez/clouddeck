use clouddeck_provider_api::{
    CellFailure, ConnectionId, Instance, KindId, ListResult, ProviderError, Selection,
};

use crate::demo_provider::requested_regions;
use crate::error::CloudDeckError;
use crate::mock::MockComputeService;
use crate::models::{EC2Instance, EC2InstanceTag};
use crate::scenario::{EC2Scenario, parse_scenario};
use crate::service::BoxError;
use crate::service_provider::{
    REGION, classify, region_cell, unsupported_kind, whole_load_failure,
};
use crate::workspace_provider::DemoFleet;

const OK_REGION: &str = "us-east-1";

pub(crate) fn scenario_data(demo: DemoFleet) -> EC2Scenario {
    let fixture = match demo {
        DemoFleet::Empty => include_str!("../testdata/ec2-scenarios/empty-fleet.json"),
        _ => include_str!("../testdata/ec2-scenarios/small-happy-path.json"),
    };
    parse_scenario(fixture).expect("bundled demo")
}

pub fn list_scenario(
    demo: DemoFleet,
    connection: &ConnectionId,
    kind: &KindId,
    selection: &Selection,
    fleet: bool,
) -> ListResult {
    if let Some(unsupported) = unsupported_kind(kind, &Selection::new()) {
        return unsupported;
    }
    match demo {
        DemoFleet::Failure => whole_load_failure(
            &simulated_error("Simulated demo load failure"),
            &Selection::new(),
        ),
        DemoFleet::Partial if fleet => partial_fleet(connection, selection),
        _ => {
            let region = if fleet {
                "fleet".to_string()
            } else {
                selection
                    .values(REGION)
                    .first()
                    .map(|value| value.as_str().to_string())
                    .unwrap_or_default()
            };
            let mut instances = instances_for(&scenario_data(demo), connection.as_str(), &region);
            if demo == DemoFleet::Large {
                instances = large_instances(instances);
            }
            if demo == DemoFleet::Huge {
                instances = huge_instances(&instances, HUGE_COUNT);
            }
            MockComputeService::sort_by_instance_id(&mut instances);
            ListResult {
                resources: instances.iter().map(Instance::from).collect(),
                failures: Vec::new(),
            }
        }
    }
}

fn partial_fleet(connection: &ConnectionId, selection: &Selection) -> ListResult {
    let data = scenario_data(DemoFleet::Partial);
    let regions = requested_regions(selection);
    let mut instances: Vec<EC2Instance> = Vec::new();
    let mut failures: Vec<(String, CloudDeckError)> = Vec::new();
    for region in &regions {
        if region == OK_REGION {
            let mut region_instances = instances_for(&data, connection.as_str(), region);
            MockComputeService::sort_by_instance_id(&mut region_instances);
            instances.extend(region_instances);
        } else {
            let message = format!("Simulated failure in {region}");
            failures.push((region.clone(), simulated_error(&message)));
        }
    }

    let had_successful_region = regions.iter().any(|region| region == OK_REGION);
    if !had_successful_region && let Some((_, error)) = failures.first() {
        return whole_load_failure(error, &Selection::new());
    }

    instances.sort_by_key(|instance| {
        (
            instance.region.as_deref().unwrap_or("").to_lowercase(),
            instance.instance_id.to_lowercase(),
        )
    });
    failures.sort_by_key(|(region, _)| region.to_lowercase());
    ListResult {
        resources: instances.iter().map(Instance::from).collect(),
        failures: failures
            .into_iter()
            .map(|(region, error)| CellFailure {
                cell: region_cell(&region),
                error: ProviderError {
                    message: error.user_message(),
                    ..ProviderError::from(error)
                },
            })
            .collect(),
    }
}

fn simulated_error(message: &str) -> CloudDeckError {
    let error: BoxError = Box::new(std::io::Error::other(message.to_string()));
    classify(&error)
}

fn instances_for(data: &EC2Scenario, profile: &str, region: &str) -> Vec<EC2Instance> {
    data.profiles
        .iter()
        .find(|candidate| candidate.name == profile)
        .map(|candidate| {
            candidate
                .regions
                .iter()
                .filter(|candidate| region == "fleet" || candidate.name == region)
                .flat_map(|candidate| {
                    candidate.instances.iter().cloned().map(|mut instance| {
                        instance.region = Some(candidate.name.clone());
                        instance
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) const HUGE_COUNT: usize = 5_000;

const HUGE_TYPES: [&str; 10] = [
    "t3.micro",
    "t3.small",
    "t3.medium",
    "t3.large",
    "m6i.large",
    "m6i.xlarge",
    "c6i.large",
    "c6i.xlarge",
    "r6i.large",
    "r6i.xlarge",
];

/// Clones `seed` cyclically into `count` instances with deterministic ids, states, types and tags.
pub(crate) fn huge_instances(seed: &[EC2Instance], count: usize) -> Vec<EC2Instance> {
    if seed.is_empty() {
        return Vec::new();
    }
    (0..count)
        .map(|n| {
            let mut instance = seed[n % seed.len()].clone();
            instance.instance_id = format!("i-huge-{n:04}");
            instance.state = Some(["running", "running", "stopped", "pending"][n % 4].to_string());
            instance.instance_type = Some(HUGE_TYPES[n % HUGE_TYPES.len()].to_string());
            instance.tags = vec![
                tag("Name", &format!("huge-{n:04}")),
                tag("Environment", ["prod", "staging", "dev"][n % 3]),
                tag(
                    "Service",
                    ["api", "web", "batch", "cache", "search", "auth", "billing"][n % 7],
                ),
                tag("Owner", ["esteban", "ana", "li", "sam", "kim"][n % 5]),
            ];
            instance
        })
        .collect()
}

fn tag(key: &str, value: &str) -> EC2InstanceTag {
    EC2InstanceTag {
        key: key.to_string(),
        value: value.to_string(),
    }
}

fn large_instances(mut instances: Vec<EC2Instance>) -> Vec<EC2Instance> {
    let seed = instances.clone();
    if seed.is_empty() {
        return instances;
    }
    for index in 4..=120 {
        let mut instance = seed[(index - 4) % seed.len()].clone();
        instance.instance_id = format!("i-demo-{index:03}");
        if let Some(name) = instance.tags.iter_mut().find(|tag| tag.key == "Name") {
            name.value = format!("demo-{index}");
        }
        instances.push(instance);
    }
    instances
}
