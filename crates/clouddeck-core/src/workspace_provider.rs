use std::sync::Arc;
use std::time::Duration;

use clouddeck_provider_api::{
    Call, ConnectionId, KindId, ListResult, Provider, Selection, ValueId,
};

use crate::aws_sdk::AwsSdkComputeService;
use crate::context::AwsProfile;
use crate::demo_scenarios::scenario_data;
use crate::fleet_compute::FleetComputeService;
use crate::mock::MockComputeService;
use crate::scoped_provider::{ScopeSettings, scoped_provider};
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
    scenario_data(demo)
        .profiles
        .into_iter()
        .map(|profile| AwsProfile {
            name: profile.name,
            region: profile.regions.first().map(|region| region.name.clone()),
        })
        .collect()
}

pub fn demo_region_names(demo: DemoFleet) -> Vec<String> {
    scenario_data(demo)
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
    let fleet = region == "fleet";
    let settings = ScopeSettings {
        request_timeout,
        fleet,
        demo,
    };
    let selection = if fleet {
        Selection::new().with(
            REGION,
            fleet_regions
                .iter()
                .map(|region| ValueId::from(region.as_str()))
                .collect(),
        )
    } else {
        Selection::new().with(REGION, vec![ValueId::from(region)])
    };
    scoped_provider(
        registry_entry(providers, demo),
        ConnectionId::from(profile),
        selection,
        settings,
    )
}

fn registry_entry(providers: &[Arc<dyn Provider>], demo: Option<DemoFleet>) -> Arc<dyn Provider> {
    let id = if demo.is_some() { "demo" } else { "aws" };
    providers
        .iter()
        .find(|provider| provider.info().id == id)
        .or_else(|| {
            providers
                .iter()
                .find(|provider| provider.info().id == "aws")
        })
        .map(Arc::clone)
        .unwrap_or_else(|| -> Arc<dyn Provider> {
            if demo.is_some() {
                Arc::new(crate::demo_provider())
            } else {
                crate::aws_provider()
            }
        })
}

pub(crate) fn routed_list(
    cx: &Call<'_>,
    connection: &ConnectionId,
    kind: &KindId,
    selection: &Selection,
    settings: &ScopeSettings,
) -> ListResult {
    let regions: Vec<String> = selection
        .values(REGION)
        .iter()
        .map(|value| value.as_str().to_string())
        .collect();
    let (region, fleet_regions) = if settings.fleet {
        ("fleet".to_string(), regions)
    } else {
        (regions.first().cloned().unwrap_or_default(), Vec::new())
    };
    // The whole-load failure cell is the empty selection, which is what the view-model LoadRequest sends.
    service_provider(service(
        connection.as_str(),
        &region,
        &fleet_regions,
        settings.request_timeout,
    ))
    .list(cx, connection, kind, &Selection::new())
}

pub(crate) fn service(
    profile: &str,
    region: &str,
    fleet_regions: &[String],
    request_timeout: Duration,
) -> Service {
    profile
        .is_empty()
        .then(no_profile_service)
        .or_else(|| {
            (region == "fleet").then(|| fleet_aws_service(profile, fleet_regions, request_timeout))
        })
        .unwrap_or_else(|| aws_service(profile, region, request_timeout))
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
