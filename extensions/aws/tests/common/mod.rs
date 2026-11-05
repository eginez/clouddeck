use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clouddeck_core::{DemoFleet, workspace_provider};
use clouddeck_provider_api::{Call, Host, LogLevel, Provider, ProviderError};

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

pub fn call() -> Call<'static> {
    Call {
        deadline: None,
        host: &HOST,
    }
}

pub fn happy_provider(providers: &[Arc<dyn Provider>], region: &str) -> Arc<dyn Provider> {
    workspace_provider(
        providers,
        "scenario-default",
        region,
        Some(DemoFleet::Happy),
        &[],
        Duration::from_secs(5),
    )
}
