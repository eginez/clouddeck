use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use clouddeck_core::fixture_fleet as fixtures;
use clouddeck_core::{
    Browse, Call, CloudDeckError, Connection, ConnectionId, Dimension, FieldDef,
    InspectorSectionDef, KindId, KindInfo, ListResult, Provider, ProviderError, ProviderInfo,
    Selection, whole_load_failure,
};
use gpui::{Bounds, Entity, MouseButton, TestAppContext, VisualTestContext, point, size};

use super::*;
use crate::view_models::InstanceListViewModel;

mod appearance;
mod layout;
mod overlays;
mod refresh;

fn failed_load(message: &str) -> ListResult {
    whole_load_failure(
        &CloudDeckError::from_service_error(&std::io::Error::other(message)),
        &Selection::new(),
    )
}

fn three_instances() -> Vec<Instance> {
    fixtures::instances(&[
        ("i-1", "api-server", "running", "t3.medium"),
        ("i-2", "bastion", "stopped", "t3.medium"),
        ("i-3", "worker", "stopped", "t3.medium"),
    ])
}

fn wait_until(cx: &mut TestAppContext, mut condition: impl FnMut(&mut TestAppContext) -> bool) {
    for _ in 0..200 {
        cx.run_until_parked();
        if condition(cx) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("condition was not satisfied within the timeout");
}

/// Opens the main window at the given size with a refreshed fleet of `instances`.
fn rooted_main_window(
    cx: &mut TestAppContext,
    instances: Vec<Instance>,
    width: f32,
    height: f32,
) -> (
    Entity<FleetState>,
    Entity<MainWindow>,
    &mut VisualTestContext,
) {
    rooted_main_window_with_service(cx, fixtures::static_provider(instances), width, height)
}

/// Counts `list` calls, failing the first with `first_call_error` when set.
struct CountingProvider {
    calls: Arc<AtomicUsize>,
    first_call_error: Option<&'static str>,
    inner: Arc<dyn Provider>,
}

impl Provider for CountingProvider {
    fn info(&self) -> ProviderInfo {
        self.inner.info()
    }
}

impl Browse for CountingProvider {
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
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        match self.first_call_error {
            Some(message) if call == 1 => whole_load_failure(
                &CloudDeckError::from_service_error(&std::io::Error::other(message)),
                selection,
            ),
            _ => self.inner.list(cx, connection, kind, selection),
        }
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

fn counting_service(
    calls: &Arc<AtomicUsize>,
    first_call_error: Option<&'static str>,
) -> Arc<dyn Provider> {
    Arc::new(CountingProvider {
        calls: calls.clone(),
        first_call_error,
        inner: fixtures::static_provider(three_instances()),
    })
}

/// Opens the main window at the given size with a refreshed fleet backed by `provider`.
fn rooted_main_window_with_service(
    cx: &mut TestAppContext,
    provider: Arc<dyn Provider>,
    width: f32,
    height: f32,
) -> (
    Entity<FleetState>,
    Entity<MainWindow>,
    &mut VisualTestContext,
) {
    let vm = InstanceListViewModel::new(provider);
    let fleet = cx.new(|_| FleetState::new(vm, "default".to_string(), true));
    fleet.update(cx, |fleet, cx| fleet.refresh(cx));
    wait_until(cx, |cx| {
        fleet.read_with(cx, |fleet, _| {
            !fleet.view_model.is_loading()
                && (fleet.view_model.last_success().is_some() || fleet.view_model.error().is_some())
        })
    });

    let main_slot: Rc<RefCell<Option<Entity<MainWindow>>>> = Rc::new(RefCell::new(None));
    let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(width), px(height)));
    let slot_for_window = main_slot.clone();
    let fleet_for_window = fleet.clone();
    let window = cx.update(|cx| {
        cx.open_window(window_options(bounds), move |window, cx| {
            gpui_component::init(cx);
            let main = cx.new(|cx| {
                let regions = vec!["us-east-1".to_string()];
                MainWindow::new(fleet_for_window, regions, window, cx)
            });
            *slot_for_window.borrow_mut() = Some(main.clone());
            cx.new(|cx| gpui_component::Root::new(main, window, cx))
        })
        .unwrap()
    });
    let cx = VisualTestContext::from_window(window.into(), cx).into_mut();
    cx.run_until_parked();
    let main = main_slot
        .borrow()
        .as_ref()
        .expect("main window entity should be created")
        .clone();
    (fleet, main, cx)
}

fn set_query(fleet: &Entity<FleetState>, query: &str, cx: &mut VisualTestContext) {
    fleet.update(cx, |fleet, cx| fleet.set_query(query, cx));
    cx.run_until_parked();
}

fn click_center_of(selector: &'static str, cx: &mut VisualTestContext) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} should render"));
    cx.simulate_click(bounds.center(), Default::default());
    cx.run_until_parked();
}

fn rendered(main: &Entity<MainWindow>, cx: &mut VisualTestContext) -> RenderedPanes {
    main.read_with(cx, |main, _| main.rendered)
}

fn rendered_overlay(main: &Entity<MainWindow>, cx: &mut VisualTestContext) -> Option<&'static str> {
    main.read_with(cx, |main, _| main.rendered_overlay)
}

fn overlay_view<T: 'static>(main: &Entity<MainWindow>, cx: &mut VisualTestContext) -> Entity<T> {
    main.read_with(cx, |main, _| main.overlay_view.clone())
        .expect("an overlay should be open")
        .downcast::<T>()
        .expect("open overlay should be the expected view")
}

fn query(fleet: &Entity<FleetState>, cx: &mut VisualTestContext) -> String {
    fleet.read_with(cx, |fleet, _| fleet.query().to_string())
}

fn search_input_text(main: &Entity<MainWindow>, cx: &mut VisualTestContext) -> String {
    let input = main.read_with(cx, |main, cx| main.search.read(cx).input.clone());
    input.read_with(cx, |input, _| input.value().to_string())
}

fn set_refresh_policy(
    main: &Entity<MainWindow>,
    auto: bool,
    on_focus: bool,
    cx: &mut VisualTestContext,
) {
    main.update(cx, |main, _| {
        main.auto_refresh = auto;
        main.refresh_on_focus = on_focus;
    });
}

fn loads(calls: &Arc<AtomicUsize>) -> usize {
    calls.load(Ordering::SeqCst)
}
