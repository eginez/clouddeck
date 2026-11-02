use std::sync::Arc;
use std::time::Duration;

use clouddeck_provider_api::{
    Call, ConnectionId, KindId, ListResult, Provider, Selection, ValueId,
};

use crate::aws_sdk::AwsSdkComputeService;
use crate::bound_provider::bound_provider;
use crate::context::AwsProfile;
use crate::fleet_compute::FleetComputeService;
use crate::mock::MockComputeService;
use crate::models::EC2Instance;
use crate::scenario::{EC2Scenario, parse_scenario};
use crate::scoped_provider::{
    DEMO_SCENARIO_KEY, FLEET_KEY, REGIONS_KEY, REQUEST_TIMEOUT_KEY, ScopeSettings, scoped_provider,
};
use crate::service::{BoxError, ComputeService};
use crate::service_provider::{REGION, service_provider};
use crate::settings::fleet_region_timeout;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemoFleet {
    Happy,
    Empty,
    Failure,
    Partial,
    Large,
}

impl DemoFleet {
    pub(crate) fn key(self) -> &'static str {
        match self {
            DemoFleet::Happy => "happy",
            DemoFleet::Empty => "empty",
            DemoFleet::Failure => "failure",
            DemoFleet::Partial => "partial",
            DemoFleet::Large => "large",
        }
    }

    pub(crate) fn from_key(key: &str) -> Option<Self> {
        [
            DemoFleet::Happy,
            DemoFleet::Empty,
            DemoFleet::Failure,
            DemoFleet::Partial,
            DemoFleet::Large,
        ]
        .into_iter()
        .find(|demo| demo.key() == key)
    }
}

type Service = Box<dyn ComputeService + Send + Sync>;

pub fn demo_profiles(demo: DemoFleet) -> Vec<AwsProfile> {
    scenario_for(demo)
        .profiles
        .into_iter()
        .map(|profile| AwsProfile {
            name: profile.name,
            region: profile.regions.first().map(|region| region.name.clone()),
        })
        .collect()
}

pub fn demo_region_names(demo: DemoFleet) -> Vec<String> {
    scenario_for(demo)
        .profiles
        .into_iter()
        .flat_map(|profile| profile.regions.into_iter().map(|region| region.name))
        .collect()
}

pub async fn verify_instance_access(profile: String, region: String) -> Result<(), BoxError> {
    AwsSdkComputeService::new(profile, region)
        .list_instances(None)
        .await
        .map(|_| ())
}

pub fn workspace_provider(
    providers: &[Arc<dyn Provider>],
    profile: &str,
    region: &str,
    demo: Option<DemoFleet>,
    fleet_regions: &[String],
    request_timeout: Duration,
) -> Arc<dyn Provider> {
    let id = if demo.is_some() { "demo" } else { "aws" };
    let entry = providers
        .iter()
        .find(|provider| provider.info().id == id)
        .or_else(|| {
            providers
                .iter()
                .find(|provider| provider.info().id == "aws")
        })
        .expect("the aws provider is registered");
    let fleet = region == "fleet";
    let regions = if fleet {
        fleet_regions.to_vec()
    } else {
        vec![region.to_string()]
    };
    let selection = Selection::new().with(
        REGION,
        regions
            .iter()
            .map(|region| ValueId::from(region.as_str()))
            .collect(),
    );
    scoped_provider(
        bound_provider(Arc::clone(entry), ConnectionId::from(profile), selection),
        ScopeSettings {
            request_timeout,
            fleet,
            demo,
            regions,
        },
    )
}

pub(crate) struct RoutedRequest {
    profile: String,
    region: String,
    regions: Vec<String>,
    request_timeout: Duration,
    demo: Option<DemoFleet>,
}

pub(crate) fn routed_request(cx: &Call<'_>, connection: &ConnectionId) -> Option<RoutedRequest> {
    let request_timeout =
        Duration::from_millis(cx.host.setting(REQUEST_TIMEOUT_KEY)?.parse().ok()?);
    let fleet = cx.host.setting(FLEET_KEY).is_some();
    let regions: Vec<String> = cx
        .host
        .setting(REGIONS_KEY)
        .unwrap_or_default()
        .split(',')
        .filter(|region| !region.is_empty())
        .map(str::to_string)
        .collect();
    let region = if fleet {
        "fleet".to_string()
    } else {
        regions.first().cloned().unwrap_or_default()
    };
    let demo = cx
        .host
        .setting(DEMO_SCENARIO_KEY)
        .and_then(|key| DemoFleet::from_key(&key));
    Some(RoutedRequest {
        profile: connection.as_str().to_string(),
        region,
        regions,
        request_timeout,
        demo,
    })
}

impl RoutedRequest {
    pub(crate) fn is_demo_fixture(&self) -> bool {
        self.demo == Some(DemoFleet::Happy) && self.region != "fleet" && !self.profile.is_empty()
    }

    pub(crate) fn list(
        &self,
        cx: &Call<'_>,
        connection: &ConnectionId,
        kind: &KindId,
    ) -> ListResult {
        service_provider(service(
            &self.profile,
            &self.region,
            self.demo,
            &self.regions,
            self.request_timeout,
        ))
        .list(cx, connection, kind, &Selection::new())
    }
}

pub(crate) fn service(
    profile: &str,
    region: &str,
    demo: Option<DemoFleet>,
    fleet_regions: &[String],
    request_timeout: Duration,
) -> Service {
    demo.map(|demo| demo_service(profile, region, demo, fleet_regions))
        .or_else(|| profile.is_empty().then(no_profile_service))
        .or_else(|| {
            (region == "fleet").then(|| fleet_aws_service(profile, fleet_regions, request_timeout))
        })
        .unwrap_or_else(|| aws_service(profile, region, request_timeout))
}

fn demo_service(profile: &str, region: &str, demo: DemoFleet, fleet_regions: &[String]) -> Service {
    match demo {
        DemoFleet::Failure => Box::new(MockComputeService::new(
            vec![],
            Some(std::io::Error::other("Simulated demo load failure").into()),
        )),
        DemoFleet::Partial if region == "fleet" => partial_service(profile, demo, fleet_regions),
        DemoFleet::Large => Box::new(MockComputeService::new(
            large_instances(instances_for(&scenario_for(demo), profile, region)),
            None,
        )),
        _ => Box::new(MockComputeService::new(
            instances_for(&scenario_for(demo), profile, region),
            None,
        )),
    }
}

fn partial_service(profile: &str, demo: DemoFleet, fleet_regions: &[String]) -> Service {
    let data = scenario_for(demo);
    let profile = profile.to_string();
    Box::new(FleetComputeService::new(
        fleet_regions.to_vec(),
        move |region| {
            if region == "us-east-1" {
                Box::new(MockComputeService::new(
                    instances_for(&data, &profile, region),
                    None,
                ))
            } else {
                Box::new(MockComputeService::new(
                    vec![],
                    Some(std::io::Error::other(format!("Simulated failure in {region}")).into()),
                ))
            }
        },
    ))
}

fn scenario_for(demo: DemoFleet) -> EC2Scenario {
    let fixture = match demo {
        DemoFleet::Empty => include_str!("../testdata/ec2-scenarios/empty-fleet.json"),
        _ => include_str!("../testdata/ec2-scenarios/small-happy-path.json"),
    };
    parse_scenario(fixture).expect("bundled demo")
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

fn no_profile_service() -> Service {
    Box::new(MockComputeService::new(vec![], Some(std::io::Error::other("No AWS profile selected. Use “Connect to AWS” to sign in and choose a profile, or explore the demo without credentials.").into())))
}

fn fleet_aws_service(
    profile: &str,
    fleet_regions: &[String],
    request_timeout: Duration,
) -> Service {
    let profile = profile.to_string();
    Box::new(FleetComputeService::with_timeout(
        fleet_regions.to_vec(),
        move |region| {
            Box::new(
                AwsSdkComputeService::new(profile.clone(), region).with_timeout(request_timeout),
            )
        },
        fleet_region_timeout(request_timeout),
    ))
}

fn aws_service(profile: &str, region: &str, request_timeout: Duration) -> Service {
    Box::new(AwsSdkComputeService::new(profile, region).with_timeout(request_timeout))
}
