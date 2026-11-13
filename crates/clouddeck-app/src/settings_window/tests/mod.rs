use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use gpui::{Entity, TestAppContext, VisualTestContext};

use super::*;
use crate::setting_rows::{match_ranges, unit_suffix};

mod pane;
mod provider;
mod search;
mod unit;

fn rooted(
    cx: &mut TestAppContext,
    settings: Settings,
) -> (Entity<SettingsWindow>, &mut VisualTestContext) {
    rooted_with(cx, settings, ProviderRegistry::default())
}

fn provider_window(
    cx: &mut TestAppContext,
) -> (Entity<SettingsWindow>, &mut VisualTestContext) {
    let dir = temp_settings_path().parent().unwrap().to_path_buf();
    rooted_with(cx, Settings::default(), provider_fixture(&dir))
}

fn rooted_with(
    cx: &mut TestAppContext,
    settings: Settings,
    providers: ProviderRegistry,
) -> (Entity<SettingsWindow>, &mut VisualTestContext) {
    let slot = Rc::new(RefCell::new(None));
    let slot_for_window = slot.clone();
    let providers = Arc::new(providers);
    let (_root, cx) = cx.add_window_view(move |window, cx| {
        gpui_component::init(cx);
        let view = cx.new(|cx| SettingsWindow::new(settings, providers, window, cx));
        *slot_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = slot
        .borrow()
        .as_ref()
        .expect("settings window should be created")
        .clone();
    view.update(cx, |view, _| view.path = temp_settings_path());
    (view, cx)
}

fn temp_settings_path() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let index = NEXT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir()
        .join(format!("clouddeck-settings-{}-{index}", std::process::id()))
        .join("settings.json")
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} should be rendered"));
    cx.simulate_click(bounds.center(), Default::default());
    cx.run_until_parked();
}

fn recorded_events(
    view: &Entity<SettingsWindow>,
    cx: &mut VisualTestContext,
) -> Rc<RefCell<Vec<SettingsEvent>>> {
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    cx.update(|_, cx| {
        cx.subscribe(view, move |_, event: &SettingsEvent, _| {
            sink.borrow_mut().push(event.clone());
        })
        .detach();
    });
    events
}

fn type_query(view: &Entity<SettingsWindow>, cx: &mut VisualTestContext, query: &'static str) {
    let search = view.read_with(cx, |view, _| view.search.clone());
    search.update_in(cx, |state, window, cx| state.set_value(query, window, cx));
    cx.run_until_parked();
}
