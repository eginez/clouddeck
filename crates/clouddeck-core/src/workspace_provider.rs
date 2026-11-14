use std::sync::Arc;
use std::time::Duration;

use clouddeck_provider_api::{ConnectionId, Provider, Selection, ValueId};
use clouddeck_provider_kit::MockComputeService;
use clouddeck_provider_kit::scoped_provider::{ScopeSettings, scoped_provider};
use clouddeck_provider_kit::service_provider::{REGION, service_provider};

pub use clouddeck_provider_kit::demo_fleet::{DemoFleet, demo_profiles, demo_region_names};

pub fn workspace_provider(
    providers: &[Arc<dyn Provider>],
    profile: &str,
    regions: &[String],
    demo: Option<DemoFleet>,
    request_timeout: Duration,
) -> Arc<dyn Provider> {
    workspace_provider_for("aws", providers, profile, regions, demo, request_timeout)
}

pub fn workspace_provider_for(
    provider_id: &str,
    providers: &[Arc<dyn Provider>],
    profile: &str,
    regions: &[String],
    demo: Option<DemoFleet>,
    request_timeout: Duration,
) -> Arc<dyn Provider> {
    let settings = ScopeSettings {
        request_timeout,
        fleet: regions.len() != 1,
        demo,
    };
    let selection = Selection::new().with(
        REGION,
        regions
            .iter()
            .map(|region| ValueId::from(region.as_str()))
            .collect(),
    );
    scoped_provider(
        registry_entry(provider_id, providers, demo),
        ConnectionId::from(profile),
        selection,
        settings,
    )
}

fn registry_entry(
    provider_id: &str,
    providers: &[Arc<dyn Provider>],
    demo: Option<DemoFleet>,
) -> Arc<dyn Provider> {
    let id = if demo.is_some() { "demo" } else { provider_id };
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

#[cfg(test)]
mod registry_entry_tests {
    use super::*;
    use clouddeck_provider_api::{
        Browse, Call, Connection, Dimension, KindInfo, ListResult, ProviderError,
    };

    struct Named(&'static str);

    impl Provider for Named {
        fn info(&self) -> clouddeck_provider_api::ProviderInfo {
            clouddeck_provider_api::ProviderInfo {
                id: self.0.to_string(),
                name: self.0.to_string(),
                version: "1.0.0".to_string(),
                connection_label: "Profile".to_string(),
                onboarding: Default::default(),
            }
        }
    }

    impl Browse for Named {
        fn connections(&self, _cx: &Call<'_>) -> Result<Vec<Connection>, ProviderError> {
            Ok(Vec::new())
        }

        fn dimensions(
            &self,
            _cx: &Call<'_>,
            _connection: &ConnectionId,
        ) -> Result<Vec<Dimension>, ProviderError> {
            Ok(Vec::new())
        }

        fn kinds(&self) -> Vec<KindInfo> {
            Vec::new()
        }

        fn list(
            &self,
            _cx: &Call<'_>,
            _connection: &ConnectionId,
            _kind: &clouddeck_provider_api::KindId,
            _selection: &Selection,
        ) -> ListResult {
            ListResult {
                resources: Vec::new(),
                failures: Vec::new(),
            }
        }

        fn console_url(
            &self,
            _connection: &ConnectionId,
            _kind: &clouddeck_provider_api::KindId,
            _resource: &clouddeck_provider_api::Instance,
        ) -> Option<String> {
            None
        }

        fn fields(
            &self,
            _kind: &clouddeck_provider_api::KindId,
        ) -> Vec<clouddeck_provider_api::FieldDef> {
            Vec::new()
        }
    }

    fn registered() -> Vec<Arc<dyn Provider>> {
        vec![Arc::new(Named("aws")), Arc::new(Named("gcp"))]
    }

    #[test]
    fn requested_provider_is_chosen_and_unknown_ids_fall_back_to_aws() {
        let providers = registered();
        let picked = registry_entry("gcp", &providers, None);
        assert_eq!(picked.info().id, "gcp");

        let fallback = registry_entry("missing", &providers, None);
        assert_eq!(fallback.info().id, "aws");
    }
}
