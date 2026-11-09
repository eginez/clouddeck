//! The `Provider` implementation: orchestration between auth, HTTP and the pure mappers.

use std::sync::Arc;
use std::time::{Duration, Instant};

use clouddeck_extension_api::{
    AuthAnswer, AuthInput, AuthMethod, AuthStatus, AuthStep, Call, CellFailure, Connection,
    ConnectionId, Dimension, ErrorKind, ExtrasSection, FieldDef, FlowId, INSTANCES, Instance,
    KindId, KindInfo, ListResult, Provider, ProviderError, ProviderInfo, ResourceId, Selection,
    SettingDef,
};
use serde::de::DeserializeOwned;

use crate::auth::{Token, TokenSource};
use crate::config::{self, Config};
use crate::fields::instance_fields;
use crate::http::{Http, Request};
use crate::map::{self, AggregatedPage, GceInstance, ProjectsPage, ZONE_DIM, ZonesPage};

const MAX_CALL: Duration = Duration::from_secs(30);
const ADC_METHOD: &str = "adc";

trait Paged: DeserializeOwned {
    fn next_token(&self) -> Option<&str>;
}

impl Paged for ProjectsPage {
    fn next_token(&self) -> Option<&str> {
        self.next_page_token.as_deref()
    }
}
impl Paged for ZonesPage {
    fn next_token(&self) -> Option<&str> {
        self.next_page_token.as_deref()
    }
}
impl Paged for AggregatedPage {
    fn next_token(&self) -> Option<&str> {
        self.next_page_token.as_deref()
    }
}

pub struct GcpProvider {
    http: Arc<dyn Http>,
    tokens: Arc<TokenSource>,
}

/// Pure: a path segment that came from outside must not be able to alter the URL.
fn safe_segment(segment: &str) -> Result<&str, ProviderError> {
    let ok = !segment.is_empty()
        && segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.:".contains(c));
    if ok {
        Ok(segment)
    } else {
        Err(ProviderError::new(
            ErrorKind::NotFound,
            format!("invalid id segment {segment:?}"),
        ))
    }
}

/// Pure: minimal percent-encoding for a page token.
fn encode_token(token: &str) -> String {
    token
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(b).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn page_url(base: &str, token: Option<&str>) -> String {
    match token {
        None => base.to_string(),
        Some(t) => {
            let sep = if base.contains('?') { '&' } else { '?' };
            format!("{base}{sep}pageToken={}", encode_token(t))
        }
    }
}

fn call_timeout(cx: &Call<'_>) -> Result<Duration, ProviderError> {
    if cx.host.cancelled() {
        return Err(ProviderError::of(ErrorKind::Timeout));
    }
    let Some(deadline) = cx.deadline else {
        return Ok(MAX_CALL);
    };
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Err(ProviderError::of(ErrorKind::Timeout));
    }
    Ok(left.min(MAX_CALL))
}

fn whole_failure(error: ProviderError) -> ListResult {
    ListResult {
        resources: Vec::new(),
        failures: vec![CellFailure {
            cell: Selection::new(),
            error,
        }],
    }
}

impl GcpProvider {
    pub fn new(http: Arc<dyn Http>, tokens: Arc<TokenSource>) -> Self {
        Self { http, tokens }
    }

    fn token(&self, cx: &Call<'_>, config: &Config) -> Result<Token, ProviderError> {
        self.tokens.token(call_timeout(cx)?, config)
    }

    fn fetch<T: DeserializeOwned>(
        &self,
        cx: &Call<'_>,
        config: &Config,
        url: &str,
    ) -> Result<T, ProviderError> {
        let timeout = call_timeout(cx)?;
        let token = self.token(cx, config)?;
        let request = Request {
            url,
            token: Some(&token.access),
            quota_project: token.quota_project.as_deref(),
            timeout,
        };
        let body = self.http.get(&request).map_err(|e| e.into_provider())?;
        serde_json::from_str(&body)
            .map_err(|e| ProviderError::new(ErrorKind::Other, format!("unexpected response: {e}")))
    }

    fn fetch_pages<T: Paged>(
        &self,
        cx: &Call<'_>,
        config: &Config,
        base: &str,
    ) -> Result<Vec<T>, ProviderError> {
        let mut pages = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let page: T = self.fetch(cx, config, &page_url(base, token.as_deref()))?;
            token = page.next_token().map(str::to_string);
            pages.push(page);
            if token.is_none() {
                return Ok(pages);
            }
        }
    }

    fn zone_dimension(
        &self,
        cx: &Call<'_>,
        config: &Config,
        project: &str,
    ) -> Result<Dimension, ProviderError> {
        let url = format!(
            "{}/compute/v1/projects/{}/zones",
            config.compute,
            safe_segment(project)?
        );
        let pages = self.fetch_pages::<ZonesPage>(cx, config, &url)?;
        let zones: Vec<_> = pages.into_iter().flat_map(|p| p.items).collect();
        Ok(map::zone_dimension(&zones))
    }

    fn try_list(
        &self,
        cx: &Call<'_>,
        project: &str,
        selection: &Selection,
    ) -> Result<ListResult, ProviderError> {
        let config = Config::from_host(cx.host)?;
        let selected = selection.values(ZONE_DIM);
        let mut failures = Vec::new();
        let mut wanted = None;
        if !selected.is_empty() {
            let dimension = self.zone_dimension(cx, &config, project)?;
            let (zones, unknown) = map::expand_selection(&dimension, selected);
            failures.extend(unknown.iter().map(|v| unknown_value(v.as_str())));
            if zones.is_empty() {
                return Ok(ListResult {
                    resources: Vec::new(),
                    failures,
                });
            }
            wanted = Some(zones);
        }
        let url = format!(
            "{}/compute/v1/projects/{}/aggregated/instances",
            config.compute,
            safe_segment(project)?
        );
        let mut resources = Vec::new();
        for page in self.fetch_pages::<AggregatedPage>(cx, &config, &url)? {
            let (found, failed) = map::map_scopes(page.items, wanted.as_ref());
            resources.extend(found);
            failures.extend(failed);
        }
        Ok(ListResult {
            resources,
            failures,
        })
    }
}

fn unknown_value(value: &str) -> CellFailure {
    let error = ProviderError::new(
        ErrorKind::NotFound,
        format!("unknown region or zone {value:?}"),
    );
    map::failure_for(value, error)
}

impl Provider for GcpProvider {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "gcp".into(),
            name: "Google Cloud".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            connection_label: "Project".into(),
        }
    }

    fn settings(&self) -> Vec<SettingDef> {
        config::setting_defs()
    }

    fn connections(&self, cx: &Call<'_>) -> Result<Vec<Connection>, ProviderError> {
        let config = Config::from_host(cx.host)?;
        let url = format!(
            "{}/v1/projects?filter=lifecycleState%3AACTIVE",
            config.resource_manager
        );
        let pages = self.fetch_pages::<ProjectsPage>(cx, &config, &url)?;
        Ok(map::map_connections(
            pages.into_iter().flat_map(|p| p.projects).collect(),
        ))
    }

    fn dimensions(&self, cx: &Call<'_>, c: &ConnectionId) -> Result<Vec<Dimension>, ProviderError> {
        let config = Config::from_host(cx.host)?;
        Ok(vec![self.zone_dimension(cx, &config, c.as_str())?])
    }

    fn kinds(&self) -> Vec<KindInfo> {
        vec![KindInfo {
            id: KindId::instances(),
            label: "Instances".into(),
        }]
    }

    fn list(&self, cx: &Call<'_>, c: &ConnectionId, kind: &KindId, sel: &Selection) -> ListResult {
        if kind.as_str() != INSTANCES {
            return ListResult::default();
        }
        self.try_list(cx, c.as_str(), sel)
            .unwrap_or_else(whole_failure)
    }

    fn describe(
        &self,
        cx: &Call<'_>,
        c: &ConnectionId,
        _kind: &KindId,
        id: &ResourceId,
    ) -> Result<Vec<ExtrasSection>, ProviderError> {
        let not_found = || ProviderError::of(ErrorKind::NotFound);
        let (zone, name) = map::split_id(id.as_str()).ok_or_else(not_found)?;
        let project = safe_segment(c.as_str())?;
        let (zone, name) = (safe_segment(zone)?, safe_segment(name)?);
        let config = Config::from_host(cx.host)?;
        let url = format!(
            "{}/compute/v1/projects/{project}/zones/{zone}/instances/{name}",
            config.compute
        );
        let gce: GceInstance = self.fetch(cx, &config, &url)?;
        Ok(map::map_instance(&gce, zone).extras)
    }

    fn console_url(&self, c: &ConnectionId, _kind: &KindId, resource: &Instance) -> Option<String> {
        let (zone, name) = map::split_id(&resource.id)?;
        Some(map::console_url(c.as_str(), zone, name))
    }

    fn fields(&self, _kind: &KindId) -> Vec<FieldDef> {
        instance_fields()
    }

    fn auth_methods(&self) -> Vec<AuthMethod> {
        vec![AuthMethod::new(
            ADC_METHOD.to_string(),
            "Application Default Credentials (gcloud)".to_string(),
        )]
    }

    fn begin_auth(
        &self,
        cx: &Call<'_>,
        method: &AuthMethod,
        _input: AuthInput,
    ) -> Result<AuthStep, ProviderError> {
        if method.id != ADC_METHOD {
            return Err(ProviderError::unsupported());
        }
        let config = Config::from_host(cx.host)?;
        self.token(cx, &config).map(|_| AuthStep::Done)
    }

    fn continue_auth(
        &self,
        _cx: &Call<'_>,
        _flow: &FlowId,
        _answer: AuthAnswer,
    ) -> Result<AuthStep, ProviderError> {
        Err(ProviderError::unsupported())
    }

    /// Only an auth failure counts as signed out; any other failure is unknown, not signed in.
    fn auth_status(&self, cx: &Call<'_>, _c: Option<&ConnectionId>) -> AuthStatus {
        let token = Config::from_host(cx.host).and_then(|config| self.token(cx, &config));
        match token {
            Ok(_) => AuthStatus::SignedIn { expires: None },
            Err(e) if e.kind == ErrorKind::Auth => AuthStatus::SignedOut,
            Err(_) => AuthStatus::Unknown,
        }
    }

    /// ADC is owned by gcloud, so this only drops the in-memory token.
    fn sign_out(&self, _cx: &Call<'_>, _c: Option<&ConnectionId>) -> Result<(), ProviderError> {
        self.tokens.forget();
        Ok(())
    }
}
