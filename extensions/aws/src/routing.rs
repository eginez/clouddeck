use std::time::Duration;

use clouddeck_core::{
    BoxError, ComputeService, FleetComputeService, MockComputeService, REGION, ScopeSettings,
    fleet_region_timeout, service_provider,
};
use clouddeck_provider_api::{Call, ConnectionId, KindId, ListResult, Selection};

use crate::aws_sdk::AwsSdkComputeService;

type Service = Box<dyn ComputeService + Send + Sync>;

pub async fn verify_instance_access(profile: String, region: String) -> Result<(), BoxError> {
    AwsSdkComputeService::new(profile, region)
        .list_instances(None)
        .await
        .map(|_| ())
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use clouddeck_core::{service_provider, workspace_provider};
    use clouddeck_provider_api::{ConnectionId, Host, KindId, LogLevel, ProviderError, Selection};

    use super::service;
    use crate::builtin_providers;

    struct NullHost;

    impl Host for NullHost {
        fn log(&self, _level: LogLevel, _message: &str) {}

        fn data_dir(&self) -> PathBuf {
            PathBuf::new()
        }

        fn setting(&self, _key: &str) -> Option<String> {
            None
        }

        fn secret(&self, _key: &str) -> Option<String> {
            None
        }

        fn set_secret(&self, _key: &str, _value: Option<&str>) -> Result<(), ProviderError> {
            Ok(())
        }

        fn open_url(&self, _url: &str) -> Result<(), ProviderError> {
            Ok(())
        }

        fn cancelled(&self) -> bool {
            false
        }
    }

    static HOST: NullHost = NullHost;

    fn call() -> clouddeck_provider_api::Call<'static> {
        clouddeck_provider_api::Call {
            deadline: None,
            host: &HOST,
        }
    }

    #[test]
    fn no_profile_listing_stays_on_the_aws_service() {
        let providers = builtin_providers(true);
        let timeout = Duration::from_secs(5);
        let fleet_regions = ["us-east-1".to_string(), "eu-west-1".to_string()];
        for region in ["us-east-1", "fleet"] {
            let expected = service_provider(service("", region, &fleet_regions, timeout)).list(
                &call(),
                &ConnectionId::from(""),
                &KindId::instances(),
                &Selection::new(),
            );
            let actual = workspace_provider(&providers, "", region, None, &fleet_regions, timeout)
                .list(
                    &call(),
                    &ConnectionId::from(""),
                    &KindId::instances(),
                    &Selection::new(),
                );
            assert_eq!(actual, expected, "no profile {region}");
        }
    }
}
