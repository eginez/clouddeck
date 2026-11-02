use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clouddeck_provider_api::{
    AuthAnswer, AuthInput, AuthMethod, AuthStatus, AuthStep, Call, Connection, ConnectionId,
    Dimension, ExtrasSection, FieldDef, FlowId, Host, InspectorSectionDef, Instance, KindId,
    KindInfo, ListResult, LogLevel, Provider, ProviderError, ProviderInfo, ResourceId, Selection,
};

use crate::workspace_provider::DemoFleet;

pub(crate) const REQUEST_TIMEOUT_KEY: &str = "clouddeck.request_timeout_ms";
pub(crate) const FLEET_KEY: &str = "clouddeck.fleet";
pub(crate) const DEMO_SCENARIO_KEY: &str = "clouddeck.demo_scenario";
pub(crate) const REGIONS_KEY: &str = "clouddeck.regions";

#[derive(Clone)]
pub(crate) struct ScopeSettings {
    pub(crate) request_timeout: Duration,
    pub(crate) fleet: bool,
    pub(crate) demo: Option<DemoFleet>,
    pub(crate) regions: Vec<String>,
}

impl ScopeSettings {
    fn get(&self, key: &str) -> Option<String> {
        match key {
            REQUEST_TIMEOUT_KEY => Some(self.request_timeout.as_millis().to_string()),
            FLEET_KEY => self.fleet.then(|| "true".to_string()),
            DEMO_SCENARIO_KEY => self.demo.map(|demo| demo.key().to_string()),
            REGIONS_KEY => Some(self.regions.join(",")),
            _ => None,
        }
    }
}

struct ScopeHost<'a> {
    host: &'a dyn Host,
    settings: &'a ScopeSettings,
}

impl Host for ScopeHost<'_> {
    fn log(&self, level: LogLevel, message: &str) {
        self.host.log(level, message);
    }

    fn data_dir(&self) -> PathBuf {
        self.host.data_dir()
    }

    fn setting(&self, key: &str) -> Option<String> {
        match key {
            REQUEST_TIMEOUT_KEY | FLEET_KEY | DEMO_SCENARIO_KEY | REGIONS_KEY => {
                self.settings.get(key)
            }
            _ => self.host.setting(key),
        }
    }

    fn secret(&self, key: &str) -> Option<String> {
        self.host.secret(key)
    }

    fn set_secret(&self, key: &str, value: Option<&str>) -> Result<(), ProviderError> {
        self.host.set_secret(key, value)
    }

    fn open_url(&self, url: &str) -> Result<(), ProviderError> {
        self.host.open_url(url)
    }

    fn cancelled(&self) -> bool {
        self.host.cancelled()
    }
}

struct ScopedProvider {
    inner: Arc<dyn Provider>,
    settings: ScopeSettings,
}

pub(crate) fn scoped_provider(
    inner: Arc<dyn Provider>,
    settings: ScopeSettings,
) -> Arc<dyn Provider> {
    Arc::new(ScopedProvider { inner, settings })
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
        connection: &ConnectionId,
    ) -> Result<Vec<Dimension>, ProviderError> {
        self.inner.dimensions(cx, connection)
    }

    fn kinds(&self) -> Vec<KindInfo> {
        self.inner.kinds()
    }

    fn list(
        &self,
        cx: &Call<'_>,
        connection: &ConnectionId,
        kind: &KindId,
        selection: &Selection,
    ) -> ListResult {
        let host = ScopeHost {
            host: cx.host,
            settings: &self.settings,
        };
        let scoped = Call {
            deadline: cx.deadline,
            host: &host,
        };
        self.inner.list(&scoped, connection, kind, selection)
    }

    fn describe(
        &self,
        cx: &Call<'_>,
        connection: &ConnectionId,
        kind: &KindId,
        id: &ResourceId,
    ) -> Result<Vec<ExtrasSection>, ProviderError> {
        self.inner.describe(cx, connection, kind, id)
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

    fn auth_status(&self, cx: &Call<'_>, connection: Option<&ConnectionId>) -> AuthStatus {
        self.inner.auth_status(cx, connection)
    }

    fn sign_out(
        &self,
        cx: &Call<'_>,
        connection: Option<&ConnectionId>,
    ) -> Result<(), ProviderError> {
        self.inner.sign_out(cx, connection)
    }

    fn console_url(
        &self,
        connection: &ConnectionId,
        kind: &KindId,
        resource: &Instance,
    ) -> Option<String> {
        self.inner.console_url(connection, kind, resource)
    }

    fn fields(&self, kind: &KindId) -> Vec<FieldDef> {
        self.inner.fields(kind)
    }

    fn inspector(&self, kind: &KindId) -> Vec<InspectorSectionDef> {
        self.inner.inspector(kind)
    }
}
