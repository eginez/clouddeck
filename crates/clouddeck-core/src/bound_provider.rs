use std::sync::Arc;

use clouddeck_provider_api::{
    AuthAnswer, AuthInput, AuthMethod, AuthStatus, AuthStep, Call, Connection, ConnectionId,
    Dimension, ExtrasSection, FieldDef, FlowId, InspectorSectionDef, Instance,
    KindId, KindInfo, ListResult, Provider, ProviderError, ProviderInfo, ResourceId, Selection,
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
