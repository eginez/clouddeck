use crate::fleet_state::FleetState;
use clouddeck_core::{
    Auth, AuthInput, AuthMethod, AuthStep, Browse, Call, ComputeServiceProvider, Connection,
    MockComputeService, Provider, ProviderError,
};
use gpui::{Entity, TestAppContext};
use std::sync::Arc;
use std::time::Duration;

pub(crate) fn plain_provider(id: &str, connections: &[&str]) -> Arc<dyn Provider> {
    Arc::new(compute(id, connections))
}

pub(crate) fn auth_provider(id: &str, connections: &[&str]) -> Arc<dyn Provider> {
    Arc::new(SignInCompute(compute(id, connections)))
}

fn compute(id: &str, connections: &[&str]) -> ComputeServiceProvider {
    ComputeServiceProvider::new(
        id,
        connections
            .iter()
            .map(|connection| Connection {
                id: (*connection).into(),
                display_name: (*connection).to_string(),
            })
            .collect(),
        Vec::new(),
        |_, _| Box::new(MockComputeService::new(Vec::new(), None)),
    )
}

/// A compute provider that also offers a browser sign-in, so the Connect window opens for it.
struct SignInCompute(ComputeServiceProvider);

impl Provider for SignInCompute {
    fn info(&self) -> clouddeck_core::ProviderInfo {
        self.0.info()
    }

    fn auth(&self) -> Option<&dyn Auth> {
        Some(self)
    }
}

impl Auth for SignInCompute {
    fn auth_methods(&self) -> Vec<AuthMethod> {
        vec![AuthMethod::new(
            "browser".to_string(),
            "Browser".to_string(),
        )]
    }

    fn begin_auth(
        &self,
        _cx: &Call<'_>,
        _method: &AuthMethod,
        _input: AuthInput,
    ) -> Result<AuthStep, ProviderError> {
        Ok(AuthStep::Done)
    }
}

impl Browse for SignInCompute {
    fn connections(&self, cx: &Call<'_>) -> Result<Vec<clouddeck_core::Connection>, ProviderError> {
        self.0.connections(cx)
    }

    fn dimensions(
        &self,
        cx: &Call<'_>,
        connection: &clouddeck_core::ConnectionId,
    ) -> Result<Vec<clouddeck_core::Dimension>, ProviderError> {
        self.0.dimensions(cx, connection)
    }

    fn kinds(&self) -> Vec<clouddeck_core::KindInfo> {
        self.0.kinds()
    }

    fn list(
        &self,
        cx: &Call<'_>,
        connection: &clouddeck_core::ConnectionId,
        kind: &clouddeck_core::KindId,
        selection: &clouddeck_core::Selection,
    ) -> clouddeck_core::ListResult {
        self.0.list(cx, connection, kind, selection)
    }

    fn console_url(
        &self,
        connection: &clouddeck_core::ConnectionId,
        kind: &clouddeck_core::KindId,
        resource: &clouddeck_core::Instance,
    ) -> Option<String> {
        self.0.console_url(connection, kind, resource)
    }

    fn fields(&self, kind: &clouddeck_core::KindId) -> Vec<clouddeck_core::FieldDef> {
        self.0.fields(kind)
    }
}

pub(crate) fn wait_until(
    cx: &mut TestAppContext,
    mut condition: impl FnMut(&mut TestAppContext) -> bool,
) {
    for _ in 0..200 {
        cx.run_until_parked();
        if condition(cx) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("condition was not satisfied within the timeout");
}

pub(crate) fn load_fleet(cx: &mut TestAppContext, state: &Entity<FleetState>) {
    state.update(cx, |state, cx| state.refresh(cx));
    wait_until(cx, |cx| {
        state.read_with(cx, |state, _| {
            !state.view_model.instances().is_empty() && !state.view_model.is_loading()
        })
    });
}
