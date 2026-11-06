use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clouddeck_provider_api::{
    AuthAnswer, AuthInput, AuthMethod, AuthStatus, AuthStep, Call, Connection, ConnectionId,
    Dimension, ExtrasSection, FieldDef, FlowId, Host, InspectorSectionDef, Instance, KindId,
    KindInfo, ListResult, LogLevel, Provider, ProviderError, ProviderInfo, ResourceId, Selection,
};

use crate::service_provider::console_url;
use crate::workspace_provider::DemoFleet;

const REQUEST_TIMEOUT_KEY: &str = "clouddeck.request_timeout_ms";
const FLEET_KEY: &str = "clouddeck.fleet";
const DEMO_SCENARIO_KEY: &str = "clouddeck.demo_scenario";
const SCOPE_TOKEN_KEY: &str = "clouddeck.scope_token";

// Derived from the process id rather than random, because every copy of this crate in the process
// (the app's and each loaded extension's) must agree on it.
fn scope_token() -> String {
    format!(
        "{:016x}",
        u64::from(std::process::id()) ^ 0x5c0_9e5c_0be0_d3f1
    )
}

fn is_scope_key(key: &str) -> bool {
    matches!(
        key,
        REQUEST_TIMEOUT_KEY | FLEET_KEY | DEMO_SCENARIO_KEY | SCOPE_TOKEN_KEY
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScopeSettings {
    pub request_timeout: Duration,
    pub fleet: bool,
    pub demo: Option<DemoFleet>,
}

impl ScopeSettings {
    pub(crate) fn setting(&self, key: &str) -> Option<String> {
        match key {
            REQUEST_TIMEOUT_KEY => Some(self.request_timeout.as_millis().to_string()),
            FLEET_KEY => Some(self.fleet.to_string()),
            DEMO_SCENARIO_KEY => self.demo.map(|demo| demo.key().to_string()),
            SCOPE_TOKEN_KEY => Some(scope_token()),
            _ => None,
        }
    }

    /// Same-process consistency check: a host that does not report the token cannot reroute listing.
    pub fn from_host(host: &dyn Host) -> Option<Self> {
        if host.setting(SCOPE_TOKEN_KEY) != Some(scope_token()) {
            return None;
        }
        Some(Self {
            request_timeout: Duration::from_millis(
                host.setting(REQUEST_TIMEOUT_KEY)?.parse().ok()?,
            ),
            fleet: host.setting(FLEET_KEY).as_deref() == Some("true"),
            demo: host
                .setting(DEMO_SCENARIO_KEY)
                .as_deref()
                .and_then(DemoFleet::from_key),
        })
    }
}

pub(crate) struct ScopeHost<'a> {
    inner: &'a dyn Host,
    settings: ScopeSettings,
}

impl<'a> ScopeHost<'a> {
    pub(crate) fn new(inner: &'a dyn Host, settings: ScopeSettings) -> Self {
        Self { inner, settings }
    }
}

impl Host for ScopeHost<'_> {
    fn log(&self, level: LogLevel, message: &str) {
        self.inner.log(level, message);
    }

    fn data_dir(&self) -> PathBuf {
        self.inner.data_dir()
    }

    fn setting(&self, key: &str) -> Option<String> {
        if is_scope_key(key) {
            return self.settings.setting(key);
        }
        self.inner.setting(key)
    }

    fn secret(&self, key: &str) -> Option<String> {
        self.inner.secret(key)
    }

    fn set_secret(&self, key: &str, value: Option<&str>) -> Result<(), ProviderError> {
        self.inner.set_secret(key, value)
    }

    fn open_url(&self, url: &str) -> Result<(), ProviderError> {
        self.inner.open_url(url)
    }

    fn cancelled(&self) -> bool {
        self.inner.cancelled()
    }
}

pub(crate) struct ScopedProvider {
    inner: Arc<dyn Provider>,
    connection: ConnectionId,
    selection: Selection,
    settings: ScopeSettings,
}

pub(crate) fn scoped_provider(
    inner: Arc<dyn Provider>,
    connection: ConnectionId,
    selection: Selection,
    settings: ScopeSettings,
) -> Arc<dyn Provider> {
    Arc::new(ScopedProvider {
        inner,
        connection,
        selection,
        settings,
    })
}

impl ScopedProvider {
    fn effective_selection<'a>(&'a self, selection: &'a Selection) -> &'a Selection {
        if selection.is_empty() {
            &self.selection
        } else {
            selection
        }
    }
}

impl Provider for ScopedProvider {
    fn info(&self) -> ProviderInfo {
        self.inner.info()
    }

    fn connections(&self, cx: &Call<'_>) -> Result<Vec<Connection>, ProviderError> {
        self.inner.connections(cx)
    }

    fn dimensions(
        &self,
        cx: &Call<'_>,
        _connection: &ConnectionId,
    ) -> Result<Vec<Dimension>, ProviderError> {
        self.inner.dimensions(cx, &self.connection)
    }

    fn kinds(&self) -> Vec<KindInfo> {
        self.inner.kinds()
    }

    fn list(
        &self,
        cx: &Call<'_>,
        _connection: &ConnectionId,
        kind: &KindId,
        selection: &Selection,
    ) -> ListResult {
        let host = ScopeHost::new(cx.host, self.settings);
        let scoped = Call {
            deadline: cx.deadline,
            host: &host,
        };
        let selection = self.effective_selection(selection);
        self.inner.list(&scoped, &self.connection, kind, selection)
    }

    fn describe(
        &self,
        cx: &Call<'_>,
        _connection: &ConnectionId,
        kind: &KindId,
        id: &ResourceId,
    ) -> Result<Vec<ExtrasSection>, ProviderError> {
        self.inner.describe(cx, &self.connection, kind, id)
    }

    fn auth_methods(&self) -> Vec<AuthMethod> {
        self.inner.auth_methods()
    }

    fn begin_auth(
        &self,
        cx: &Call<'_>,
        method: &AuthMethod,
        input: AuthInput,
    ) -> Result<AuthStep, ProviderError> {
        let input = AuthInput {
            connection: Some(self.connection.clone()),
            ..input
        };
        self.inner.begin_auth(cx, method, input)
    }

    fn continue_auth(
        &self,
        cx: &Call<'_>,
        flow: &FlowId,
        answer: AuthAnswer,
    ) -> Result<AuthStep, ProviderError> {
        self.inner.continue_auth(cx, flow, answer)
    }

    fn auth_status(&self, cx: &Call<'_>, _connection: Option<&ConnectionId>) -> AuthStatus {
        self.inner.auth_status(cx, Some(&self.connection))
    }

    fn sign_out(
        &self,
        cx: &Call<'_>,
        _connection: Option<&ConnectionId>,
    ) -> Result<(), ProviderError> {
        self.inner.sign_out(cx, Some(&self.connection))
    }

    fn console_url(
        &self,
        _connection: &ConnectionId,
        kind: &KindId,
        resource: &Instance,
    ) -> Option<String> {
        console_url(kind, resource)
    }

    fn fields(&self, kind: &KindId) -> Vec<FieldDef> {
        self.inner.fields(kind)
    }

    fn inspector(&self, kind: &KindId) -> Vec<InspectorSectionDef> {
        self.inner.inspector(kind)
    }
}
