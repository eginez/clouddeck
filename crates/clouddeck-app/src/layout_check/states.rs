//! Named UI states built in fresh windows.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use clouddeck_core::{
    ComputeService, DirectoryScenarioCatalog, EC2Instance, EC2InstanceTag, MockComputeService,
    ProfileInput, RegionScope, ScenarioService, TestScenarioKind,
};
use gpui::{
    AppContext, Bounds, Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext, point,
    px, size,
};

use crate::fleet_state::FleetState;
use crate::main_window::{MainWindow, window_options};
use crate::view_models::InstanceListViewModel;
use crate::welcome_window::WelcomeWindow;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum State {
    /// Fleet loaded with nothing selected.
    MainIdle,
    /// Fleet loaded with the first visible row selected.
    MainRowSelected,
    /// Fleet loaded with no instances.
    MainEmpty,
    /// Fleet load failed outright.
    MainFailure,
    /// Two regions where one fails.
    MainPartial,
    /// A fleet of 120 instances.
    MainLarge,
    /// Search field focused and empty, with recent searches listed.
    SearchFocusedEmpty,
    /// Search field focused with "st" typed, showing field suggestions.
    SearchTypingSt,
    /// Search field focused with an unknown field term typed, showing the error hint.
    SearchError,
    /// Profile picker opened with cmd-p over the idle main window.
    OverlayProfile,
    /// Region picker opened with cmd-shift-r over the idle main window.
    OverlayRegion,
    /// Command palette opened with cmd-shift-p over the idle main window.
    OverlayPalette,
    /// Failure popover opened by clicking the partial-failure bar.
    OverlayFailure,
    /// Row context menu opened by right-clicking the first row.
    OverlayRowMenu,
    /// First-run welcome window listing three profiles.
    Welcome,
}

/// The fixture scenario catalog shipped with clouddeck-core.
fn catalog() -> DirectoryScenarioCatalog {
    DirectoryScenarioCatalog::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../clouddeck-core/testdata/ec2-scenarios"
    ))
}

/// Resolves a fixture-backed scenario kind.
fn resolve(kind: TestScenarioKind) -> ScenarioService {
    ScenarioService::for_kind(kind, &catalog()).expect("fixture scenario should load")
}

/// 120 instances cloned from the first small-happy-path instance.
fn large_fleet() -> Vec<EC2Instance> {
    let resolved = resolve(TestScenarioKind::SmallHappyPath);
    let scenario = resolved.scenario().expect("fixture-backed scenario");
    let (profile, region) = scenario
        .default_selection()
        .expect("fixture has a default selection");
    let template = scenario
        .instances(Some(&profile), &region)
        .expect("fixture instances should load")
        .into_iter()
        .next()
        .expect("fixture has at least one instance");
    (0..120)
        .map(|n| {
            let mut instance = template.clone();
            instance.instance_id = format!("i-large-{n:03}");
            instance.state = Some(if n % 2 == 0 { "running" } else { "stopped" }.to_string());
            instance.tags.retain(|tag| tag.key != "Name");
            instance.tags.push(EC2InstanceTag {
                key: "Name".to_string(),
                value: format!("large-instance-{n:03}"),
            });
            instance
        })
        .collect()
}

/// The compute service and region list that back `state`.
fn service_for(state: State) -> (Box<dyn ComputeService + Send + Sync>, Vec<String>) {
    let us_east = vec!["us-east-1".to_string()];
    match state {
        State::MainIdle
        | State::MainRowSelected
        | State::SearchFocusedEmpty
        | State::SearchTypingSt
        | State::SearchError
        | State::OverlayProfile
        | State::OverlayRegion
        | State::OverlayPalette
        | State::OverlayRowMenu => (
            Box::new(resolve(TestScenarioKind::SmallHappyPath).into_service()),
            us_east,
        ),
        State::MainEmpty => (Box::new(MockComputeService::new(vec![], None)), us_east),
        State::MainFailure => (
            Box::new(MockComputeService::new(
                vec![],
                Some(std::io::Error::other("Simulated demo load failure").into()),
            )),
            us_east,
        ),
        State::MainPartial | State::OverlayFailure => (
            Box::new(resolve(TestScenarioKind::MultiRegionPartialFailure).into_service()),
            vec![
                "us-east-1".to_string(),
                "eu-west-1".to_string(),
                "ap-southeast-2".to_string(),
            ],
        ),
        State::MainLarge => (
            Box::new(MockComputeService::new(large_fleet(), None)),
            us_east,
        ),
        State::Welcome => unreachable!("the welcome window has no fleet service"),
    }
}

/// The three profiles the welcome state lists.
fn welcome_profiles() -> Vec<ProfileInput> {
    [
        ("prod-readonly", "111122223333", "us-east-1"),
        ("staging-admin", "111122224444", "us-west-2"),
        ("sandbox", "777788889999", "eu-west-1"),
    ]
    .map(|(name, account_id, region)| ProfileInput {
        name: name.to_string(),
        account_id: Some(account_id.to_string()),
        region: Some(region.to_string()),
    })
    .to_vec()
}

/// Opens the welcome window at the given size and returns its context.
fn open_welcome(width: f32, height: f32, cx: &mut TestAppContext) -> &mut VisualTestContext {
    let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(width), px(height)));
    let window = cx.update(|cx| {
        cx.open_window(window_options(bounds), |window, cx| {
            gpui_component::init(cx);
            let welcome = cx.new(|cx| WelcomeWindow::new(welcome_profiles(), window, cx));
            cx.new(|cx| gpui_component::Root::new(welcome, window, cx))
        })
        .unwrap()
    });
    let cx = VisualTestContext::from_window(window.into(), cx).into_mut();
    cx.run_until_parked();
    cx
}

/// Sends `keystrokes` to the window and runs until parked.
fn press(cx: &mut VisualTestContext, keystrokes: &str) {
    cx.simulate_keystrokes(keystrokes);
    cx.run_until_parked();
}

/// Clicks the centre of the element with `selector`.
fn click_center(cx: &mut VisualTestContext, selector: &'static str) {
    let center = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} should be rendered"))
        .center();
    cx.simulate_click(center, Modifiers::none());
    cx.run_until_parked();
}

/// Right-clicks the centre of the element with `selector`.
fn right_click_center(cx: &mut VisualTestContext, selector: &'static str) {
    let center = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} should be rendered"))
        .center();
    cx.simulate_mouse_down(center, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(center, MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
}

/// Pumps the test dispatcher until `condition` holds, panicking after about a second.
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

/// Focuses the search field with cmd-f, then types `text`.
fn focus_search(cx: &mut VisualTestContext, text: &str) {
    cx.simulate_keystrokes("cmd-f");
    cx.run_until_parked();
    cx.simulate_input(text);
    cx.run_until_parked();
}

/// Builds `state` in a fresh window; the welcome state returns no fleet or main window.
pub(crate) fn build<'a>(
    state: State,
    width: f32,
    height: f32,
    cx: &'a mut TestAppContext,
) -> (
    Option<Entity<FleetState>>,
    Option<Entity<MainWindow>>,
    &'a mut VisualTestContext,
) {
    if state == State::Welcome {
        return (None, None, open_welcome(width, height, cx));
    }

    let (service, regions) = service_for(state);
    let vm = InstanceListViewModel::new(service);
    let fleet = cx.new(|_| FleetState::new(vm, "default".to_string(), true));
    fleet.update(cx, |fleet, cx| fleet.refresh(cx));
    wait_until(cx, |cx| {
        fleet.read_with(cx, |fleet, _| {
            !fleet.view_model.is_loading()
                && (fleet.view_model.last_success().is_some() || fleet.view_model.error().is_some())
        })
    });

    if state == State::SearchFocusedEmpty {
        fleet.update(cx, |fleet, _| {
            fleet.recent_searches = [
                "state:stopped age:>30d",
                "owner:esteban",
                "has:public-ip type:t3.*",
                "billing prod",
            ]
            .map(String::from)
            .to_vec();
        });
    }

    let main_slot: Rc<RefCell<Option<Entity<MainWindow>>>> = Rc::new(RefCell::new(None));
    let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(width), px(height)));
    let slot_for_window = main_slot.clone();
    let fleet_for_window = fleet.clone();
    let window = cx.update(|cx| {
        cx.open_window(window_options(bounds), move |window, cx| {
            gpui_component::init(cx);
            let scope = RegionScope::all(&regions);
            let main = cx.new(|cx| MainWindow::new(fleet_for_window, regions, scope, window, cx));
            if state == State::SearchFocusedEmpty {
                let search = main.read(cx).search_field();
                search.update(cx, |search, cx| search.focus(window, cx));
            }
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

    // Mirrors AppShell's fleet observer, which copies partial failures into the window.
    let weak = main.downgrade();
    cx.update(|_, app| {
        app.observe(&fleet, move |observed, app| {
            let failures = observed.read(app).view_model.partial_failures().to_vec();
            let _ = weak.update(app, |main, cx| {
                main.failures = failures;
                cx.notify();
            });
        })
        .detach();
    });
    let failures = fleet.read_with(cx, |fleet, _| fleet.view_model.partial_failures().to_vec());
    main.update(cx, |main, cx| {
        main.failures = failures;
        cx.notify();
    });
    cx.run_until_parked();

    if state == State::MainRowSelected {
        let key = fleet.read_with(cx, |fleet, _| {
            fleet
                .visible_rows()
                .first()
                .expect("fleet has a visible row")
                .id()
        });
        fleet.update(cx, |fleet, cx| fleet.select(Some(key), cx));
        cx.run_until_parked();
    }

    match state {
        State::SearchTypingSt => focus_search(cx, "st"),
        State::SearchError => focus_search(cx, "colour:x"),
        State::OverlayProfile => press(cx, "cmd-p"),
        State::OverlayRegion => press(cx, "cmd-shift-r"),
        State::OverlayPalette => press(cx, "cmd-shift-p"),
        State::OverlayFailure => click_center(cx, "bar.failure"),
        State::OverlayRowMenu => {
            let key = fleet.read_with(cx, |fleet, _| {
                fleet
                    .visible_rows()
                    .first()
                    .expect("fleet has a visible row")
                    .id()
            });
            // debug_bounds only takes 'static selectors; the leak lasts for one test.
            let selector: &'static str = Box::leak(format!("table.row.{key}").into_boxed_str());
            right_click_center(cx, selector);
        }
        _ => {}
    }

    (Some(fleet), Some(main), cx)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [State; 6] = [
        State::MainIdle,
        State::MainRowSelected,
        State::MainEmpty,
        State::MainFailure,
        State::MainPartial,
        State::MainLarge,
    ];

    #[gpui::test]
    fn every_state_renders_root_and_statusbar(cx: &mut TestAppContext) {
        for state in ALL {
            let (_fleet, _main, window_cx) = build(state, 1280.0, 760.0, cx);
            assert!(
                window_cx.debug_bounds("main.root").is_some(),
                "{state:?}: main.root"
            );
            assert!(
                window_cx.debug_bounds("main.statusbar").is_some(),
                "{state:?}: main.statusbar"
            );
        }
    }

    #[gpui::test]
    fn selected_row_shows_the_inspector(cx: &mut TestAppContext) {
        let (_fleet, _main, cx) = build(State::MainRowSelected, 1280.0, 760.0, cx);

        assert!(cx.debug_bounds("main.inspector").is_some());
    }

    #[gpui::test]
    fn idle_window_has_no_inspector(cx: &mut TestAppContext) {
        let (_fleet, _main, cx) = build(State::MainIdle, 1280.0, 760.0, cx);

        assert!(cx.debug_bounds("main.inspector").is_none());
    }

    #[gpui::test]
    fn empty_and_failure_show_the_center_state(cx: &mut TestAppContext) {
        for state in [State::MainEmpty, State::MainFailure] {
            let (_fleet, _main, window_cx) = build(state, 1280.0, 760.0, cx);
            assert!(
                window_cx.debug_bounds("main.center-state").is_some(),
                "{state:?}: main.center-state"
            );
        }
    }

    #[gpui::test]
    fn large_fleet_lists_every_instance(cx: &mut TestAppContext) {
        let (fleet, _main, cx) = build(State::MainLarge, 1280.0, 760.0, cx);
        let fleet = fleet.expect("fleet state has a fleet");

        assert_eq!(
            fleet.read_with(cx, |fleet, _| fleet.visible_rows().len()),
            120
        );
    }

    #[gpui::test]
    fn partial_failure_shows_the_failure_bar(cx: &mut TestAppContext) {
        let (_fleet, _main, cx) = build(State::MainPartial, 1280.0, 760.0, cx);

        assert!(cx.debug_bounds("bar.failure").is_some());
    }

    #[gpui::test]
    fn focused_empty_search_shows_recents_and_hides_the_hint(cx: &mut TestAppContext) {
        let (_fleet, _main, cx) = build(State::SearchFocusedEmpty, 1280.0, 760.0, cx);

        assert!(cx.debug_bounds("search.recent").is_some());
        assert!(cx.debug_bounds("search.hint").is_none());
    }

    #[gpui::test]
    fn typing_st_shows_suggestions_for_the_query(cx: &mut TestAppContext) {
        let (fleet, _main, cx) = build(State::SearchTypingSt, 1280.0, 760.0, cx);
        let fleet = fleet.expect("fleet state has a fleet");

        assert!(cx.debug_bounds("search.suggestions").is_some());
        assert_eq!(fleet.read_with(cx, |fleet, _| fleet.query().to_string()), "st");
    }

    #[gpui::test]
    fn unknown_field_shows_the_search_error(cx: &mut TestAppContext) {
        let (_fleet, _main, cx) = build(State::SearchError, 1280.0, 760.0, cx);

        assert!(cx.debug_bounds("search.error").is_some());
    }

    #[gpui::test]
    fn profile_overlay_shows_its_panel(cx: &mut TestAppContext) {
        let (_fleet, _main, cx) = build(State::OverlayProfile, 1280.0, 760.0, cx);

        assert!(cx.debug_bounds("main.overlay.profile").is_some());
    }

    #[gpui::test]
    fn region_overlay_shows_its_panel(cx: &mut TestAppContext) {
        let (_fleet, _main, cx) = build(State::OverlayRegion, 1280.0, 760.0, cx);

        assert!(cx.debug_bounds("main.overlay.region").is_some());
    }

    #[gpui::test]
    fn palette_overlay_shows_its_panel(cx: &mut TestAppContext) {
        let (_fleet, _main, cx) = build(State::OverlayPalette, 1280.0, 760.0, cx);

        assert!(cx.debug_bounds("main.overlay.palette").is_some());
    }

    #[gpui::test]
    fn failure_overlay_shows_its_panel(cx: &mut TestAppContext) {
        let (_fleet, _main, cx) = build(State::OverlayFailure, 1280.0, 760.0, cx);

        assert!(cx.debug_bounds("main.overlay.failure").is_some());
    }

    #[gpui::test]
    fn row_menu_overlay_shows_its_panel(cx: &mut TestAppContext) {
        let (_fleet, _main, cx) = build(State::OverlayRowMenu, 1280.0, 760.0, cx);

        assert!(cx.debug_bounds("main.overlay.rowmenu").is_some());
    }

    #[gpui::test]
    fn welcome_lists_the_profiles_with_search(cx: &mut TestAppContext) {
        let (fleet, main, cx) = build(State::Welcome, 1280.0, 760.0, cx);

        assert!(fleet.is_none() && main.is_none());
        assert!(cx.debug_bounds("welcome.search").is_some());
        assert!(cx.debug_bounds("welcome.profile.2").is_some());
    }
}
