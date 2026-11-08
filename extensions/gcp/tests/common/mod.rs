#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clouddeck_extension_api::{Call, Host, LogLevel, ProviderError};
use clouddeck_gcp::{GcpProvider, Http, HttpError, Request, TokenSource, parse_adc};

pub fn fixture(name: &str) -> String {
    match name {
        "adc_authorized_user.json" => include_str!("../fixtures/adc_authorized_user.json"),
        "adc_service_account.json" => include_str!("../fixtures/adc_service_account.json"),
        "aggregated_empty.json" => include_str!("../fixtures/aggregated_empty.json"),
        "aggregated_instances.json" => include_str!("../fixtures/aggregated_instances.json"),
        "projects.json" => include_str!("../fixtures/projects.json"),
        "token.json" => include_str!("../fixtures/token.json"),
        "zones.json" => include_str!("../fixtures/zones.json"),
        other => panic!("unknown gcp test fixture: {other}"),
    }
    .to_string()
}

/// Serves recorded JSON by URL shape and remembers every URL it was asked for.
pub struct FakeHttp {
    pub urls: Mutex<Vec<String>>,
    pub bearer: &'static str,
    pub fail_token: bool,
    pub network_down: bool,
}

impl Default for FakeHttp {
    fn default() -> Self {
        Self {
            urls: Mutex::default(),
            bearer: "ya29.fake-access-token",
            fail_token: false,
            network_down: false,
        }
    }
}

fn not_found(url: &str) -> HttpError {
    HttpError {
        status: Some(404),
        timed_out: false,
        message: format!("no fixture for {url}"),
    }
}

/// Looks `.../zones/{zone}/instances/{name}` up in the aggregated fixture.
fn instance(url: &str) -> Option<String> {
    let (zone, name) = url.split_once("/zones/")?.1.split_once("/instances/")?;
    let all: serde_json::Value =
        serde_json::from_str(&fixture("aggregated_instances.json")).ok()?;
    let list = all["items"][format!("zones/{zone}")]["instances"].as_array()?;
    list.iter()
        .find(|i| i["name"] == name)
        .map(|i| i.to_string())
}

impl FakeHttp {
    fn route(&self, url: &str) -> Result<String, HttpError> {
        let demo_prod = url.contains("/projects/demo-prod/");
        let body = match url {
            u if u.contains("/v1/projects?") => fixture("projects.json"),
            u if u.ends_with("/zones") => fixture("zones.json"),
            u if u.contains("/instances/") => return instance(u).ok_or_else(|| not_found(u)),
            u if u.ends_with("/aggregated/instances") && demo_prod => {
                fixture("aggregated_instances.json")
            }
            u if u.ends_with("/aggregated/instances") => fixture("aggregated_empty.json"),
            _ => return Err(not_found(url)),
        };
        Ok(body)
    }

    pub fn count(&self, needle: &str) -> usize {
        self.urls
            .lock()
            .unwrap()
            .iter()
            .filter(|u| u.contains(needle))
            .count()
    }
}

impl Http for FakeHttp {
    fn get(&self, request: &Request<'_>) -> Result<String, HttpError> {
        self.urls.lock().unwrap().push(request.url.to_string());
        assert_eq!(request.token, Some(self.bearer));
        self.route(request.url)
    }

    fn post_form(
        &self,
        url: &str,
        form: &[(&str, &str)],
        _: Duration,
    ) -> Result<String, HttpError> {
        self.urls.lock().unwrap().push(url.to_string());
        assert!(form.contains(&("grant_type", "refresh_token")));
        if self.fail_token {
            return Err(HttpError {
                status: Some(400),
                timed_out: false,
                message: "invalid_grant".into(),
            });
        }
        if self.network_down {
            return Err(HttpError {
                status: None,
                timed_out: false,
                message: "connection reset".into(),
            });
        }
        Ok(fixture("token.json"))
    }
}

pub fn provider_with(http: Arc<FakeHttp>, adc: Result<String, ProviderError>) -> GcpProvider {
    let shared: Arc<dyn Http> = http;
    let load = Box::new(move || parse_adc(&adc.clone()?));
    let tokens = Arc::new(TokenSource::new(Arc::clone(&shared), load));
    GcpProvider::new(shared, tokens)
}

pub fn provider(http: Arc<FakeHttp>) -> GcpProvider {
    provider_with(http, Ok(fixture("adc_authorized_user.json")))
}

pub struct TestHost;

impl Host for TestHost {
    fn log(&self, _: LogLevel, _: &str) {}
    fn data_dir(&self) -> PathBuf {
        PathBuf::new()
    }
    fn setting(&self, _: &str) -> Option<String> {
        None
    }
    fn secret(&self, _: &str) -> Option<String> {
        None
    }
    fn set_secret(&self, _: &str, _: Option<&str>) -> Result<(), ProviderError> {
        Ok(())
    }
    fn open_url(&self, _: &str) -> Result<(), ProviderError> {
        Ok(())
    }
    fn cancelled(&self) -> bool {
        false
    }
}

/// A host that serves the given settings and nothing else.
pub struct SettingsHost(pub Vec<(&'static str, &'static str)>);

impl Host for SettingsHost {
    fn log(&self, _: LogLevel, _: &str) {}
    fn data_dir(&self) -> PathBuf {
        PathBuf::new()
    }
    fn setting(&self, key: &str) -> Option<String> {
        self.0
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.to_string())
    }
    fn secret(&self, _: &str) -> Option<String> {
        None
    }
    fn set_secret(&self, _: &str, _: Option<&str>) -> Result<(), ProviderError> {
        Ok(())
    }
    fn open_url(&self, _: &str) -> Result<(), ProviderError> {
        Ok(())
    }
    fn cancelled(&self) -> bool {
        false
    }
}

pub fn call(host: &dyn Host) -> Call<'_> {
    Call {
        deadline: None,
        host,
    }
}
