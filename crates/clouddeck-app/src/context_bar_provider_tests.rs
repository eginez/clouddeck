use std::{cell::RefCell, rc::Rc};

use clouddeck_core::fixture_fleet as fleet;
use gpui::{
    AppContext, Bounds, Entity, TestAppContext, VisualTestContext, WindowBounds, WindowOptions,
    point, px, size,
};
use gpui_component::Root;

use crate::context_bar::{ContextBar, ContextBarEvent};
use crate::fleet_state::FleetState;
use crate::test_support::load_fleet;
use crate::ui_record::recorded;
use crate::view_models::InstanceListViewModel;

type Events = Rc<RefCell<Vec<ContextBarEvent>>>;

fn loaded_state(cx: &mut TestAppContext) -> Entity<FleetState> {
    let instances = vec![
        fleet::instance("i-1", "api-server", "running", "t3.medium"),
        fleet::instance("i-2", "bastion", "stopped", "t3.medium"),
    ];
    let vm = InstanceListViewModel::new(fleet::static_provider(instances));
    let state = cx.new(|_| FleetState::new(vm, "prod-readonly".to_string(), true));
    load_fleet(cx, &state);
    state
}

fn open_bar(
    cx: &mut TestAppContext,
    sign_in: bool,
) -> (Entity<ContextBar>, Events, &mut VisualTestContext) {
    let state = loaded_state(cx);
    let bar = cx.new(|cx| ContextBar::new(state, cx));
    bar.update(cx, |bar, _| bar.sign_in = sign_in);
    let events: Events = Default::default();
    let sink = events.clone();
    let subscribed = bar.clone();
    cx.update(|cx| {
        cx.subscribe(&subscribed, move |_, event: &ContextBarEvent, _| {
            sink.borrow_mut().push(*event)
        })
        .detach();
    });
    let root_bar = bar.clone();
    let bounds = Bounds::new(point(px(0.), px(0.)), size(px(1280.0), px(760.0)));
    let window = cx
        .update(|cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| {
                    gpui_component::init(cx);
                    cx.new(|cx| Root::new(root_bar, window, cx))
                },
            )
        })
        .unwrap();
    let cx = VisualTestContext::from_window(window.into(), cx).into_mut();
    cx.run_until_parked();
    (bar, events, cx)
}

fn assert_rendered(cx: &mut VisualTestContext, keys: &[&'static str]) {
    for key in keys {
        assert!(cx.debug_bounds(key).is_some(), "{key} should render");
    }
}

fn assert_absent(cx: &mut VisualTestContext, keys: &[&'static str]) {
    for key in keys {
        assert!(cx.debug_bounds(key).is_none(), "{key} should not render");
    }
}

fn click(cx: &mut VisualTestContext, key: &'static str) {
    let bounds = cx
        .debug_bounds(key)
        .unwrap_or_else(|| panic!("{key} should render"));
    cx.simulate_click(bounds.center(), Default::default());
    cx.run_until_parked();
}

#[gpui::test]
fn default_bar_marks_aws_and_offers_the_profile_and_region_pickers(cx: &mut TestAppContext) {
    let (_, _, cx) = open_bar(cx, false);

    assert_rendered(cx, &["bar.aws", "bar.profile", "bar.region"]);
    assert_absent(cx, &["bar.sign-in"]);
    let mark = cx.update(|_, app| recorded(app, "bar.aws"));
    assert_eq!(mark.and_then(|rec| rec.text).as_deref(), Some("AWS"));
}

#[gpui::test]
fn provider_mark_shows_the_active_provider_and_opens_the_picker(cx: &mut TestAppContext) {
    let (bar, events, cx) = open_bar(cx, false);
    bar.update(cx, |bar, cx| {
        bar.provider_short_name = "GCP".to_string();
        cx.notify();
    });
    cx.run_until_parked();

    let mark = cx.update(|_, app| recorded(app, "bar.aws"));
    assert_eq!(mark.and_then(|rec| rec.text).as_deref(), Some("GCP"));
    click(cx, "bar.aws");
    assert_eq!(*events.borrow(), vec![ContextBarEvent::OpenProviderPicker]);
}

#[gpui::test]
fn sign_in_state_replaces_the_profile_and_region_pickers(cx: &mut TestAppContext) {
    let (_, events, cx) = open_bar(cx, true);

    assert_rendered(cx, &["bar.sign-in", "bar.aws"]);
    assert_absent(cx, &["bar.profile", "bar.region"]);
    assert!(cx.update(|_, app| recorded(app, "bar.profile")).is_none());
    click(cx, "bar.sign-in");
    assert_eq!(*events.borrow(), vec![ContextBarEvent::SignIn]);
}
