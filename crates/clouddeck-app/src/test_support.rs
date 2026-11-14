use crate::fleet_state::FleetState;
use clouddeck_core::release::sign::generate_keypair;
use clouddeck_core::{
    Auth, AuthInput, AuthMethod, AuthStep, Browse, Call, ComputeServiceProvider, Connection,
    MockComputeService, Provider, ProviderError,
};
use gpui::{
    AppContext, Context, Entity, EventEmitter, Render, TestAppContext, VisualTestContext, Window,
};
use gpui_component::Root;
use gpui_component::input::InputState;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

pub(crate) fn plain_provider(id: &str, connections: &[&str]) -> Arc<dyn Provider> {
    Arc::new(compute(id, connections))
}

pub(crate) fn auth_provider(id: &str, connections: &[&str]) -> Arc<dyn Provider> {
    Arc::new(SignInCompute(compute(id, connections), None))
}

/// Like `auth_provider`, but its `identity` check answers with `identity`.
pub(crate) fn identified_provider(
    id: &str,
    connections: &[&str],
    identity: &'static str,
) -> Arc<dyn Provider> {
    Arc::new(SignInCompute(compute(id, connections), Some(identity)))
}

fn compute(id: &str, connections: &[&str]) -> ComputeServiceProvider {
    ComputeServiceProvider::new(
        id,
        connections
            .iter()
            .map(|connection| Connection {
                id: (*connection).into(),
                display_name: (*connection).to_string(),
                detail: None,
                tags: Vec::new(),
            })
            .collect(),
        Vec::new(),
        |_, _| Box::new(MockComputeService::new(Vec::new(), None)),
    )
}

/// A compute provider that also offers a browser sign-in, so the onboarding window opens for it.
struct SignInCompute(ComputeServiceProvider, Option<&'static str>);

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

    fn identity(
        &self,
        _cx: &Call<'_>,
        _connection: &clouddeck_core::ConnectionId,
        _selection: &clouddeck_core::Selection,
    ) -> Result<String, ProviderError> {
        self.1
            .map(str::to_string)
            .ok_or_else(ProviderError::unsupported)
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

/// A gzipped tar whose only entry is `clouddeck` with the given contents.
pub fn tar_gz_with_binary(content: &[u8]) -> Vec<u8> {
    let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
        Vec::new(),
        flate2::Compression::default(),
    ));
    let mut header = tar::Header::new_gnu();
    header.set_path("clouddeck").unwrap();
    header.set_size(content.len() as u64);
    header.set_mode(0o755);
    header.set_cksum();
    builder.append(&header, content).unwrap();
    let mut plugin = tar::Header::new_gnu();
    plugin
        .set_path("extensions-bundled/aws/extension.toml")
        .unwrap();
    plugin.set_size(1);
    plugin.set_mode(0o644);
    plugin.set_cksum();
    builder.append(&plugin, &b"m"[..]).unwrap();
    builder.into_inner().unwrap().finish().unwrap()
}

/// A keypair plus an `old` executable in a temp dir kept alive by the returned guard.
pub fn updater_sandbox() -> (String, String, PathBuf, tempfile::TempDir) {
    let (secret, public) = generate_keypair();
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("clouddeck");
    std::fs::write(&exe, b"old").unwrap();
    (secret, public, exe, dir)
}

/// Opens `build`'s view in a test window with its input set to `query`, recording every event it emits.
pub fn open_with_query<'a, V, E>(
    cx: &'a mut TestAppContext,
    query: &'static str,
    build: impl FnOnce(&mut Window, &mut Context<V>) -> V + 'static,
    input: fn(&V) -> &Entity<InputState>,
) -> (Entity<V>, Rc<RefCell<Vec<E>>>, &'a mut VisualTestContext)
where
    V: Render + EventEmitter<E> + 'static,
    E: Clone + 'static,
{
    let slot = Rc::new(RefCell::new(None));
    let slot_for_window = slot.clone();
    let (_root, cx) = cx.add_window_view(move |window, cx| {
        gpui_component::init(cx);
        let view = cx.new(|cx| build(window, cx));
        view.update(cx, |view, cx| {
            input(view).update(cx, |state, cx| state.set_value(query, window, cx));
        });
        *slot_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = slot
        .borrow()
        .as_ref()
        .expect("view entity should be created")
        .clone();
    let events: Rc<RefCell<Vec<E>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    cx.update(|_, cx| {
        cx.subscribe(&view, move |_, event: &E, _| {
            sink.borrow_mut().push(event.clone())
        })
        .detach();
    });
    (view, events, cx)
}
