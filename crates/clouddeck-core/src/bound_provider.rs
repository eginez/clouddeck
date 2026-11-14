use std::sync::Arc;

use clouddeck_provider_api::{
    Auth, AuthAnswer, AuthInput, AuthMethod, AuthStatus, AuthStep, Browse, Call, Connection,
    ConnectionId, Dimension, ExtrasSection, FieldDef, FlowId, InspectorSectionDef, Instance,
    KindId, KindInfo, ListResult, NoAuth, Onboarding, Provider, ProviderError, ProviderInfo,
    ResourceId, Selection,
};

use crate::service_provider::console_url;

pub struct BoundProvider {
    inner: Arc<dyn Provider>,
    connection: ConnectionId,
    selection: Selection,
}

pub fn bound_provider(
    inner: Arc<dyn Provider>,
    connection: ConnectionId,
    selection: Selection,
) -> Arc<dyn Provider> {
    Arc::new(BoundProvider {
        inner,
        connection,
        selection,
    })
}

impl Provider for BoundProvider {
    fn info(&self) -> ProviderInfo {
        self.inner.info()
    }

    fn onboarding(&self, cx: &Call<'_>) -> Onboarding {
        self.inner.onboarding(cx)
    }

    fn auth(&self) -> Option<&dyn Auth> {
        self.inner.auth().map(|_| self as &dyn Auth)
    }
}

impl Browse for BoundProvider {
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
        let selection = if selection.is_empty() {
            &self.selection
        } else {
            selection
        };
        self.inner.list(cx, &self.connection, kind, selection)
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

impl Auth for BoundProvider {
    fn auth_methods(&self) -> Vec<AuthMethod> {
        self.inner.auth().unwrap_or(&NoAuth).auth_methods()
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
        self.inner
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
        self.inner
            .auth()
            .unwrap_or(&NoAuth)
            .continue_auth(cx, flow, answer)
    }

    fn auth_status(&self, cx: &Call<'_>, _connection: Option<&ConnectionId>) -> AuthStatus {
        self.inner
            .auth()
            .unwrap_or(&NoAuth)
            .auth_status(cx, Some(&self.connection))
    }

    fn sign_out(
        &self,
        cx: &Call<'_>,
        _connection: Option<&ConnectionId>,
    ) -> Result<(), ProviderError> {
        self.inner
            .auth()
            .unwrap_or(&NoAuth)
            .sign_out(cx, Some(&self.connection))
    }
}
