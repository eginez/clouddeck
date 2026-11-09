//! Runs blocking provider calls off the gpui foreground executor with the app's host.

use std::path::PathBuf;

use clouddeck_core::{Call, Host, LogLevel, ProviderError, context_path};
use gpui::AsyncApp;

pub(crate) async fn off_thread<T: Send + 'static>(
    cx: &AsyncApp,
    work: impl FnOnce() -> T + Send + 'static,
) -> T {
    cx.background_executor().spawn(async move { work() }).await
}

pub(crate) fn with_call<T>(work: impl FnOnce(&Call<'_>) -> T) -> T {
    let host = AppHost;
    work(&Call {
        deadline: None,
        host: &host,
    })
}

struct AppHost;

impl Host for AppHost {
    fn log(&self, level: LogLevel, message: &str) {
        eprintln!("clouddeck {level:?}: {message}");
    }

    fn data_dir(&self) -> PathBuf {
        context_path()
            .parent()
            .map(PathBuf::from)
            .unwrap_or_default()
    }

    fn setting(&self, _key: &str) -> Option<String> {
        None
    }

    fn secret(&self, _key: &str) -> Option<String> {
        None
    }

    fn set_secret(&self, _key: &str, _value: Option<&str>) -> Result<(), ProviderError> {
        Err(ProviderError::unsupported())
    }

    fn open_url(&self, _url: &str) -> Result<(), ProviderError> {
        Err(ProviderError::unsupported())
    }

    fn cancelled(&self) -> bool {
        false
    }
}
