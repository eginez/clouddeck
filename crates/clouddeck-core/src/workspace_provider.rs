use std::sync::Arc;
use std::time::Duration;

use clouddeck_provider_api::{ConnectionId, Provider, Selection, ValueId};

use crate::context::AwsProfile;
use crate::demo_scenarios::scenario_data;
use crate::mock::MockComputeService;
use crate::scoped_provider::{ScopeSettings, scoped_provider};
use crate::service_provider::{REGION, service_provider};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemoFleet {
    Happy,
    Empty,
    Failure,
    Partial,
    Large,
    Huge,
}

impl DemoFleet {
    pub(crate) fn key(self) -> &'static str {
        match self {
            DemoFleet::Happy => "happy",
            DemoFleet::Empty => "empty",
            DemoFleet::Failure => "failure",
            DemoFleet::Partial => "partial",
            DemoFleet::Large => "large",
            DemoFleet::Huge => "huge",
        }
    }

    pub(crate) fn from_key(key: &str) -> Option<Self> {
        [
            DemoFleet::Happy,
            DemoFleet::Empty,
            DemoFleet::Failure,
            DemoFleet::Partial,
            DemoFleet::Large,
            DemoFleet::Huge,
        ]
        .into_iter()
        .find(|demo| demo.key() == key)
    }
}

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
                service_provider(Box::new(MockComputeService::new(
                    vec![],
                    Some(std::io::Error::other("No AWS provider is registered.").into()),
                )))
            }
        })
}
