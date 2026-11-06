use std::sync::Arc;
use std::time::Duration;

use clouddeck_provider_api::{ConnectionId, Provider, Selection, ValueId};
use clouddeck_provider_kit::scoped_provider::{ScopeSettings, scoped_provider};
use clouddeck_provider_kit::service_provider::{REGION, service_provider};
use clouddeck_provider_kit::MockComputeService;

pub use clouddeck_provider_kit::demo_fleet::{DemoFleet, demo_profiles, demo_region_names};

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
