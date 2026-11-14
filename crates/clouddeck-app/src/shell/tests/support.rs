use super::*;
use clouddeck_core::Browse;
use gpui::{TestAppContext, VisualTestContext};

pub(super) fn compute_provider(id: &str) -> std::sync::Arc<dyn Provider> {
    std::sync::Arc::new(clouddeck_core::ComputeServiceProvider::new(
        id,
        vec![clouddeck_core::Connection {
            id: "default".into(),
            display_name: "default".to_string(),
            detail: None,
            tags: Vec::new(),
        }],
        Vec::new(),
        |_, _| Box::new(clouddeck_core::MockComputeService::new(Vec::new(), None)),
    ))
}

pub(super) struct SignedOut(pub(super) std::sync::Arc<dyn Provider>);

impl Provider for SignedOut {
    fn info(&self) -> clouddeck_core::ProviderInfo {
        self.0.info()
    }
}

impl Browse for SignedOut {
    fn connections(
        &self,
        _cx: &clouddeck_core::Call<'_>,
    ) -> Result<Vec<clouddeck_core::Connection>, clouddeck_core::ProviderError> {
        Err(clouddeck_core::ProviderError::of(
            clouddeck_core::ErrorKind::Auth,
        ))
    }

    fn dimensions(
        &self,
        cx: &clouddeck_core::Call<'_>,
        connection: &clouddeck_core::ConnectionId,
    ) -> Result<Vec<clouddeck_core::Dimension>, clouddeck_core::ProviderError> {
        self.0.dimensions(cx, connection)
    }

    fn kinds(&self) -> Vec<clouddeck_core::KindInfo> {
        self.0.kinds()
    }

    fn list(
        &self,
        cx: &clouddeck_core::Call<'_>,
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

pub(super) fn install_providers(
    cx: &mut TestAppContext,
    providers: Vec<std::sync::Arc<dyn Provider>>,
) {
    cx.update(|cx| {
        let registry = clouddeck_core::extension_host::app_registry(
            None,
            Path::new("/nonexistent-clouddeck-extensions"),
            Path::new("/nonexistent-clouddeck-state"),
            providers,
        );
        cx.set_global(crate::app_providers::AppProviders::new(registry));
    });
}

pub(super) fn install_registry(cx: &mut TestAppContext, gcp: std::sync::Arc<dyn Provider>) {
    install_providers(cx, vec![compute_provider("aws"), gcp]);
}

pub(super) fn bare_ec2(region: &str, instance_id: &str) -> clouddeck_core::EC2Instance {
    clouddeck_core::EC2Instance {
        instance_id: instance_id.to_string(),
        region: Some(region.to_string()),
        instance_type: None,
        state: None,
        launch_time: None,
        public_ip_address: None,
        private_ip_address: None,
        vpc_id: None,
        subnet_id: None,
        security_groups: Vec::new(),
        tags: Vec::new(),
        availability_zone: None,
        image_id: None,
        key_name: None,
        architecture: None,
        state_transition_reason: None,
        iam_instance_profile_arn: None,
        private_dns_name: None,
        public_dns_name: None,
    }
}

/// Mounts the shell the way `main.rs` does: gpui-component is initialised
/// and the shell is wrapped in a `Root`, so its widgets have a theme.
pub(super) fn add_rooted_shell(
    cx: &mut TestAppContext,
    build: impl FnOnce(&mut Window, &mut Context<AppShell>) -> AppShell + 'static,
) -> (Entity<AppShell>, &mut VisualTestContext) {
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_for_window = slot.clone();
    let (_root, cx) = cx.add_window_view(move |window, cx| {
        gpui_component::init(cx);
        let shell = cx.new(|cx| build(window, cx));
        *slot_for_window.borrow_mut() = Some(shell.clone());
        gpui_component::Root::new(shell, window, cx)
    });
    let shell = slot
        .borrow()
        .as_ref()
        .expect("rooted shell entity should be created")
        .clone();
    (shell, cx)
}

/// Polls `run_until_parked` + short sleeps until `selector` renders,
/// mirroring the startup-load pattern the other AppShell tests use
/// (the demo load crosses onto the shared tokio runtime, so one
/// `run_until_parked()` can return before the rows land).
pub(super) fn wait_for_row(cx: &mut VisualTestContext, selector: &str) {
    for _ in 0..200 {
        cx.run_until_parked();
        let static_selector: &'static str = Box::leak(selector.to_string().into_boxed_str());
        if cx.debug_bounds(static_selector).is_some() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("row {selector} did not render within the timeout");
}

pub(super) fn temp_ui_state_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("clouddeck-shell-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir.join("ui-state.json")
}

pub(super) fn test_aws_provider() -> std::sync::Arc<dyn clouddeck_core::Provider> {
    std::sync::Arc::new(clouddeck_core::ComputeServiceProvider::new(
        "aws",
        Vec::new(),
        Vec::new(),
        |_, _| Box::new(clouddeck_core::MockComputeService::new(Vec::new(), None)),
    ))
}

/// `AppShell::new_demo`'s startup load resolves on the shared tokio
/// runtime (a real OS thread pool outside gpui's deterministic test
/// executor), so poll with bounded, short real sleeps rather than
/// assuming a single `run_until_parked()` lands the result.
///
/// The closure takes a fresh `&mut VisualTestContext` reborrow on each
/// tick, so it can call `debug_bounds` (a `VisualTestContext`-only
/// method) or read entity state without holding a conflicting borrow
/// on `cx` itself for the whole loop.
pub(super) fn wait_until(
    cx: &mut gpui::VisualTestContext,
    label: &str,
    mut condition: impl FnMut(&mut gpui::VisualTestContext) -> bool,
) {
    for _ in 0..200 {
        cx.run_until_parked();
        if condition(cx) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("timed out waiting for: {label}");
}

/// Polls until the switched-in context has finished loading and the
/// fleet shows its instances.
pub(super) fn wait_until_context_settled(
    cx: &mut gpui::VisualTestContext,
    shell: &gpui::Entity<AppShell>,
) {
    wait_until(cx, "context load to settle", |cx| {
        shell.read_with(cx, |shell, cx| {
            let vm = &shell.fleet.read(cx).view_model;
            !vm.is_loading() && !vm.instances().is_empty()
        })
    });
}

/// Emits a main window event the way an overlay does, then lets the
/// shell's subscription and any spawned loads run.
pub(super) fn emit_main_window_event(
    cx: &mut gpui::VisualTestContext,
    shell: &gpui::Entity<AppShell>,
    event: MainWindowEvent,
) {
    let main_window = shell.read_with(cx, |shell, _| shell.main_window.clone());
    main_window.update(cx, |_, cx| cx.emit(event));
    cx.run_until_parked();
}

pub(super) struct Unauthenticated(pub(super) std::sync::Arc<dyn Provider>);

impl Provider for Unauthenticated {
    fn info(&self) -> clouddeck_core::ProviderInfo {
        self.0.info()
    }
}

impl Browse for Unauthenticated {
    fn connections(
        &self,
        _cx: &clouddeck_core::Call<'_>,
    ) -> Result<Vec<clouddeck_core::Connection>, clouddeck_core::ProviderError> {
        Err(clouddeck_core::ProviderError::of(
            clouddeck_core::ErrorKind::Auth,
        ))
    }

    fn dimensions(
        &self,
        cx: &clouddeck_core::Call<'_>,
        connection: &clouddeck_core::ConnectionId,
    ) -> Result<Vec<clouddeck_core::Dimension>, clouddeck_core::ProviderError> {
        self.0.dimensions(cx, connection)
    }

    fn kinds(&self) -> Vec<clouddeck_core::KindInfo> {
        self.0.kinds()
    }

    fn list(
        &self,
        _cx: &clouddeck_core::Call<'_>,
        _connection: &clouddeck_core::ConnectionId,
        _kind: &clouddeck_core::KindId,
        _selection: &clouddeck_core::Selection,
    ) -> clouddeck_core::ListResult {
        clouddeck_core::ListResult {
            resources: Vec::new(),
            failures: vec![clouddeck_core::CellFailure {
                cell: clouddeck_core::Selection::new(),
                error: clouddeck_core::ProviderError::of(clouddeck_core::ErrorKind::Auth),
            }],
        }
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
