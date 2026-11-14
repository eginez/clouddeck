//! AWS provider: EC2 listing over the AWS SDK, IAM Identity Center sign-in and
//! caller identity checks. Built as a cdylib extension and loaded by the app as a bundled extension.

mod aws_auth;
mod aws_identity;
mod aws_sdk;
mod aws_sso_login;
mod routing;

use std::sync::Arc;

use clouddeck_extension_api::{
    Auth, AuthAnswer, AuthInput, AuthMethod, AuthStatus, AuthStep, Browse, Call, Config,
    Connection, ConnectionId, Dimension, Extension, ExtrasSection, FieldDef, FlowId,
    InspectorSectionDef, Instance, KindId, KindInfo, ListResult, NoAuth, NoConfig, Onboarding,
    Provider, ProviderError, ProviderInfo, ResourceId, Selection, SettingDef, register_extension,
};

pub use aws_auth::aws_provider;
pub use aws_sdk::{AwsSdkComputeService, Ec2Page, Ec2PageSource};

pub struct Aws {
    provider: Arc<dyn Provider>,
}

impl Extension for Aws {
    fn new() -> Self {
        Aws {
            provider: aws_provider(),
        }
    }

    fn provider(&self, id: &str) -> Option<Box<dyn Provider>> {
        (id == "aws").then(|| Box::new(Shared(self.provider.clone())) as Box<dyn Provider>)
    }
}

// The host asks for a provider on every request, and sign-in flows live in the provider, so
// each request must reach the same instance.
struct Shared(Arc<dyn Provider>);

impl Provider for Shared {
    fn info(&self) -> ProviderInfo {
        self.0.info()
    }

    fn onboarding(&self, cx: &Call<'_>) -> Onboarding {
        self.0.onboarding(cx)
    }

    fn auth(&self) -> Option<&dyn Auth> {
        self.0.auth().map(|_| self as &dyn Auth)
    }

    fn config(&self) -> Option<&dyn Config> {
        self.0.config().map(|_| self as &dyn Config)
    }
}

impl Browse for Shared {
    fn connections(&self, cx: &Call<'_>) -> Result<Vec<Connection>, ProviderError> {
        self.0.connections(cx)
    }

    fn dimensions(
        &self,
        cx: &Call<'_>,
        connection: &ConnectionId,
    ) -> Result<Vec<Dimension>, ProviderError> {
        self.0.dimensions(cx, connection)
    }

    fn kinds(&self) -> Vec<KindInfo> {
        self.0.kinds()
    }

    fn list(
        &self,
        cx: &Call<'_>,
        connection: &ConnectionId,
        kind: &KindId,
        selection: &Selection,
    ) -> ListResult {
        self.0.list(cx, connection, kind, selection)
    }

    fn describe(
        &self,
        cx: &Call<'_>,
        connection: &ConnectionId,
        kind: &KindId,
        id: &ResourceId,
    ) -> Result<Vec<ExtrasSection>, ProviderError> {
        self.0.describe(cx, connection, kind, id)
    }

    fn console_url(
        &self,
        connection: &ConnectionId,
        kind: &KindId,
        resource: &Instance,
    ) -> Option<String> {
        self.0.console_url(connection, kind, resource)
    }

    fn fields(&self, kind: &KindId) -> Vec<FieldDef> {
        self.0.fields(kind)
    }

    fn inspector(&self, kind: &KindId) -> Vec<InspectorSectionDef> {
        self.0.inspector(kind)
    }
}

impl Auth for Shared {
    fn auth_methods(&self) -> Vec<AuthMethod> {
        self.0.auth().unwrap_or(&NoAuth).auth_methods()
    }

    fn begin_auth(
        &self,
        cx: &Call<'_>,
        method: &AuthMethod,
        input: AuthInput,
    ) -> Result<AuthStep, ProviderError> {
        self.0
            .auth()
            .unwrap_or(&NoAuth)
            .begin_auth(cx, method, input)
    }

    fn continue_auth(
        &self,
        cx: &Call<'_>,
        flow: &FlowId,
        answer: AuthAnswer,
    ) -> Result<AuthStep, ProviderError> {
        self.0
            .auth()
            .unwrap_or(&NoAuth)
            .continue_auth(cx, flow, answer)
    }

    fn auth_status(&self, cx: &Call<'_>, connection: Option<&ConnectionId>) -> AuthStatus {
        self.0.auth().unwrap_or(&NoAuth).auth_status(cx, connection)
    }

    fn sign_out(
        &self,
        cx: &Call<'_>,
        connection: Option<&ConnectionId>,
    ) -> Result<(), ProviderError> {
        self.0.auth().unwrap_or(&NoAuth).sign_out(cx, connection)
    }

    fn identity(
        &self,
        cx: &Call<'_>,
        connection: &ConnectionId,
        selection: &Selection,
    ) -> Result<String, ProviderError> {
        self.0
            .auth()
            .unwrap_or(&NoAuth)
            .identity(cx, connection, selection)
    }

    fn check_access(
        &self,
        cx: &Call<'_>,
        connection: &ConnectionId,
        kind: &KindId,
        selection: &Selection,
    ) -> Result<(), ProviderError> {
        self.0
            .auth()
            .unwrap_or(&NoAuth)
            .check_access(cx, connection, kind, selection)
    }
}

impl Config for Shared {
    fn settings(&self) -> Vec<SettingDef> {
        self.0.config().unwrap_or(&NoConfig).settings()
    }

    fn settings_changed(&self, cx: &Call<'_>, changed: &[String]) {
        self.0
            .config()
            .unwrap_or(&NoConfig)
            .settings_changed(cx, changed)
    }
}

register_extension!(Aws);

pub fn builtin_providers() -> Vec<Arc<dyn Provider>> {
    vec![aws_provider()]
}
