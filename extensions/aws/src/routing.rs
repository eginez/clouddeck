use std::time::Duration;

use clouddeck_provider_api::{Call, ConnectionId, KindId, ListResult, Selection};
use clouddeck_provider_kit::{
    AwsFiles, BoxError, ComputeService, FleetComputeService, MockComputeService, REGION,
    ScopeSettings, fleet_region_timeout, service_provider,
};

use crate::aws_auth::{REQUEST_TIMEOUT_SETTING, aws_files};
use crate::aws_sdk::AwsSdkComputeService;

type Service = Box<dyn ComputeService + Send + Sync>;

pub async fn verify_instance_access(
    profile: String,
    region: String,
    files: AwsFiles,
) -> Result<(), BoxError> {
    AwsSdkComputeService::new(profile, region)
        .with_files(files)
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
        request_timeout(cx, settings.request_timeout),
        aws_files(cx),
    ))
    .list(cx, connection, kind, &Selection::new())
}

fn request_timeout(cx: &Call<'_>, fallback: Duration) -> Duration {
    cx.host
        .setting(REQUEST_TIMEOUT_SETTING)
        .and_then(|seconds| seconds.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(fallback)
}

pub(crate) fn service(
    profile: &str,
    region: &str,
    fleet_regions: &[String],
    request_timeout: Duration,
    files: AwsFiles,
) -> Service {
    profile
        .is_empty()
        .then(no_profile_service)
        .or_else(|| {
            (region == "fleet")
                .then(|| fleet_aws_service(profile, fleet_regions, request_timeout, files.clone()))
        })
        .unwrap_or_else(|| aws_service(profile, region, request_timeout, files))
}

fn no_profile_service() -> Service {
    Box::new(MockComputeService::new(vec![], Some(std::io::Error::other("No AWS profile selected. Use “Connect to AWS” to sign in and choose a profile, or explore the demo without credentials.").into())))
}

fn fleet_aws_service(
    profile: &str,
    fleet_regions: &[String],
    request_timeout: Duration,
    files: AwsFiles,
) -> Service {
    let profile = profile.to_string();
    Box::new(FleetComputeService::with_timeout(
        fleet_regions.to_vec(),
        move |region| {
            Box::new(
                AwsSdkComputeService::new(profile.clone(), region)
                    .with_timeout(request_timeout)
                    .with_files(files.clone()),
            )
        },
        fleet_region_timeout(request_timeout),
    ))
}

fn aws_service(profile: &str, region: &str, request_timeout: Duration, files: AwsFiles) -> Service {
    Box::new(
        AwsSdkComputeService::new(profile, region)
            .with_timeout(request_timeout)
            .with_files(files),
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    use clouddeck_core::{AwsFiles, workspace_provider};
    use clouddeck_provider_api::{ConnectionId, Host, KindId, LogLevel, ProviderError, Selection};
    use clouddeck_provider_kit::service_provider;

    use super::{aws_files, request_timeout, service};
    use crate::aws_auth::{CONFIG_DIR_SETTING, CONFIG_FILE_SETTING, REQUEST_TIMEOUT_SETTING};
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
        let mut providers = builtin_providers();
        providers.push(Arc::new(clouddeck_core::demo_provider()));
        let timeout = Duration::from_secs(5);
        let fleet_regions = ["us-east-1".to_string(), "eu-west-1".to_string()];
        let cases = [
            ("us-east-1", vec!["us-east-1".to_string()]),
            ("fleet", fleet_regions.to_vec()),
        ];
        for (region, regions) in cases {
            let expected = service_provider(service(
                "",
                region,
                &fleet_regions,
                timeout,
                AwsFiles::default(),
            ))
            .list(
                &call(),
                &ConnectionId::from(""),
                &KindId::instances(),
                &Selection::new(),
            );
            let actual = workspace_provider(&providers, "", &regions, None, timeout).list(
                &call(),
                &ConnectionId::from(""),
                &KindId::instances(),
                &Selection::new(),
            );
            assert_eq!(actual, expected, "no profile {region}");
        }
    }

    struct SettingsHost(Vec<(&'static str, &'static str)>);

    impl Host for SettingsHost {
        fn log(&self, _level: LogLevel, _message: &str) {}

        fn data_dir(&self) -> PathBuf {
            PathBuf::new()
        }

        fn setting(&self, key: &str) -> Option<String> {
            self.0
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| value.to_string())
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

    #[test]
    fn custom_config_file_and_timeout_reach_the_listing() {
        let host = SettingsHost(vec![
            (CONFIG_FILE_SETTING, "/srv/aws/profiles"),
            (REQUEST_TIMEOUT_SETTING, "5"),
        ]);
        let cx = clouddeck_provider_api::Call {
            deadline: None,
            host: &host,
        };
        assert_eq!(
            request_timeout(&cx, Duration::from_secs(30)),
            Duration::from_secs(5)
        );
        assert_eq!(aws_files(&cx).config, PathBuf::from("/srv/aws/profiles"));
    }

    #[test]
    fn timeout_falls_back_to_the_scope_setting_and_default_config_is_unset() {
        let host = SettingsHost(vec![
            (CONFIG_FILE_SETTING, "~/.aws/config"),
            (REQUEST_TIMEOUT_SETTING, "not-a-number"),
        ]);
        let cx = clouddeck_provider_api::Call {
            deadline: None,
            host: &host,
        };
        assert_eq!(
            request_timeout(&cx, Duration::from_secs(45)),
            Duration::from_secs(45)
        );
        assert_eq!(aws_files(&cx), AwsFiles::default());
    }

    #[test]
    fn config_folder_reaches_the_listing_files() {
        let host = SettingsHost(vec![(CONFIG_DIR_SETTING, "/srv/aws")]);
        let cx = clouddeck_provider_api::Call {
            deadline: None,
            host: &host,
        };
        let files = aws_files(&cx);
        assert_eq!(files.config, PathBuf::from("/srv/aws/config"));
        assert_eq!(files.credentials, PathBuf::from("/srv/aws/credentials"));
    }
}
