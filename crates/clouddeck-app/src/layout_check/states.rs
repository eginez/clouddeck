//! Named UI states built in fresh windows.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use clouddeck_core::about::{AboutInfo, PluginOrigin, PluginRow, PluginStatus};
use clouddeck_core::{ProfileInput, Provider, RegionScope, TestScenarioKind, fixture_fleet};
use gpui::{AppContext, Bounds, Entity, TestAppContext, VisualTestContext, point, px, size};

use clouddeck_core::extension_manager::plugins::{Action, PluginsViewModel, Tab};

use crate::about_window;
use crate::fleet_state::FleetState;
use crate::main_window::{MainWindow, window_options};
use crate::plugins_window;
use crate::plugins_window::fixtures::{
    ToggleOnlyActions, bundled_set, inert_manager, inert_roots, refused_set, with_installed_set,
};
use crate::render_state::{DEFAULT_ROW_MENU_ANCHOR, apply_render_state};
use crate::tokens::{ActivePalette, Palette};
use crate::view_models::InstanceListViewModel;
use crate::welcome_window::WelcomeWindow;

pub(crate) use crate::render_state::State;

/// The provider and region list that back `state`.
fn service_for(state: State) -> (Arc<dyn Provider>, Vec<String>) {
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
            fixture_fleet::scenario_provider(TestScenarioKind::SmallHappyPath),
            us_east,
        ),
        State::MainEmpty => (fixture_fleet::static_provider(vec![]), us_east),
        State::MainFailure => (
            fixture_fleet::failing_provider("Simulated demo load failure"),
            us_east,
        ),
        State::MainPartial | State::OverlayFailure => (
            fixture_fleet::scenario_provider(TestScenarioKind::MultiRegionPartialFailure),
            vec![
                "us-east-1".to_string(),
                "eu-west-1".to_string(),
                "ap-southeast-2".to_string(),
            ],
        ),
        State::MainLarge => (
            fixture_fleet::static_provider(fixture_fleet::large_fleet()),
            us_east,
        ),
        State::Welcome
        | State::AboutLight
        | State::AboutDark
        | State::AboutRefusedPlugin
        | State::AboutLongPluginList
        | State::AboutLongPluginName
        | State::PluginsLight
        | State::PluginsDark
        | State::PluginsInstalled
        | State::PluginsRefused
        | State::PluginsEmptySearch
        | State::PluginsAvailableEmpty
        | State::PluginsRestart
        | State::PluginsNarrow => {
            unreachable!("only main window states have a fleet service")
        }
    }
}

/// The three profiles the welcome state lists.
fn welcome_profiles() -> Vec<ProfileInput> {
    [
        ("prod-readonly", "222222222222", "us-east-1"),
        ("staging-admin", "333333333333", "us-west-2"),
        ("sandbox", "444444444444", "eu-west-1"),
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

/// The About window's build info and plugin rows for `state`, and whether it uses the dark palette.
fn about_fixture(state: State) -> (AboutInfo, bool) {
    let (plugins, dark) = match state {
        State::AboutLight => (three_plugins(), false),
        State::AboutDark => (three_plugins(), true),
        State::AboutRefusedPlugin => (refused_plugin(), false),
        State::AboutLongPluginList => (long_plugin_list(), false),
        State::AboutLongPluginName => (long_plugin_name(), false),
        _ => unreachable!("{state:?} is not an About state"),
    };
    let commit = "0123456789abcdef0123456789abcdef01234567";
    let info = AboutInfo {
        name: "CloudDeck".into(),
        version: "0.2.1".into(),
        commit: commit.into(),
        version_short: "0.2.1+stable.01234567".into(),
        version_full: format!("0.2.1+stable.{commit}"),
        plugins,
    };
    (info, dark)
}

fn plugin(name: &str, version: &str, origin: PluginOrigin, status: PluginStatus) -> PluginRow {
    PluginRow {
        id: name.to_lowercase(),
        name: name.into(),
        version: version.into(),
        origin,
        enabled: true,
        status,
    }
}

fn demo_plugin() -> PluginRow {
    plugin("Demo", "0.2.1", PluginOrigin::Bundled, PluginStatus::Ok)
}

fn three_plugins() -> Vec<PluginRow> {
    vec![
        demo_plugin(),
        plugin(
            "Amazon Web Services",
            "0.2.1",
            PluginOrigin::Bundled,
            PluginStatus::Ok,
        ),
        plugin("Toy", "0.1.0", PluginOrigin::Installed, PluginStatus::Ok),
    ]
}

fn refused_plugin() -> Vec<PluginRow> {
    vec![
        demo_plugin(),
        plugin("AWS", "0.2.1", PluginOrigin::Bundled, PluginStatus::Ok),
        plugin(
            "Old",
            "0.1.0",
            PluginOrigin::Installed,
            PluginStatus::Refused("Rebuild Old for CloudDeck (built for API 5)".into()),
        ),
    ]
}

fn long_plugin_list() -> Vec<PluginRow> {
    let mut plugins = vec![demo_plugin()];
    plugins.extend((1..=23).map(|n| {
        plugin(
            &format!("Extension {n:02}"),
            "0.1.0",
            PluginOrigin::Installed,
            PluginStatus::Ok,
        )
    }));
    plugins
}

fn long_plugin_name() -> Vec<PluginRow> {
    vec![
        demo_plugin(),
        plugin(
            "Amazon Web Services Enterprise Compliance and Governance Toolkit",
            "0.2.1",
            PluginOrigin::Bundled,
            PluginStatus::Ok,
        ),
    ]
}

/// Opens a fresh About window for `state`; the window is not the app's single About window.
fn open_about(state: State, cx: &mut TestAppContext) -> &mut VisualTestContext {
    let (info, dark) = about_fixture(state);
    cx.update(|app| app.set_global(ActivePalette(Palette::for_dark(dark))));
    let window = cx.update(|app| {
        gpui_component::init(app);
        about_window::open_window_for(app, info).expect("about window opens")
    });
    let cx = VisualTestContext::from_window(window.into(), cx).into_mut();
    cx.run_until_parked();
    cx
}

/// The Plugins window's model for `state`, with the selection, query, tab or restart it shows.
fn plugins_model(state: State) -> PluginsViewModel {
    let entries = match state {
        State::PluginsRefused => refused_set(),
        State::PluginsRestart => with_installed_set(),
        _ => bundled_set(),
    };
    let mut model = plugins_window::model_for(entries);
    match state {
        State::PluginsInstalled | State::PluginsNarrow => model.select(Some("demo")),
        State::PluginsRefused => model.select(Some("old")),
        State::PluginsEmptySearch => model.set_query("zzz-no-match"),
        State::PluginsAvailableEmpty => model.set_tab(Tab::Available),
        State::PluginsRestart => {
            model
                .act(&ToggleOnlyActions, "toy", Action::Disable)
                .expect("disabling toy succeeds");
            model.select(Some("demo"));
        }
        _ => {}
    }
    model
}

/// Opens a fresh Plugins window for `state`; the window is not the app's single Plugins window.
fn open_plugins(
    state: State,
    width: f32,
    height: f32,
    cx: &mut TestAppContext,
) -> &mut VisualTestContext {
    let dark = state == State::PluginsDark;
    cx.update(|app| app.set_global(ActivePalette(Palette::for_dark(dark))));
    let window = cx.update(|app| {
        gpui_component::init(app);
        plugins_window::open_window_for(
            app,
            plugins_model(state),
            inert_manager(),
            inert_roots(),
            (width, height),
        )
        .expect("plugins window opens")
    });
    let cx = VisualTestContext::from_window(window.into(), cx).into_mut();
    cx.run_until_parked();
    cx
}

/// Builds `state` in a fresh window; the welcome, About and Plugins states return no fleet or main window.
pub(crate) fn build(
    state: State,
    width: f32,
    height: f32,
    cx: &mut TestAppContext,
) -> (
    Option<Entity<FleetState>>,
    Option<Entity<MainWindow>>,
    &mut VisualTestContext,
) {
    if state == State::Welcome {
        return (None, None, open_welcome(width, height, cx));
    }
    if state.is_about() {
        return (None, None, open_about(state, cx));
    }
    if state.is_plugins() {
        return (None, None, open_plugins(state, width, height, cx));
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

    let main_slot: Rc<RefCell<Option<Entity<MainWindow>>>> = Rc::new(RefCell::new(None));
    let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(width), px(height)));
    let slot_for_window = main_slot.clone();
    let fleet_for_window = fleet.clone();
    // debug_bounds is never cleared between frames, so a change after the first frame
    // (a focus change, or a selection that moves the table's columns) would leave stale
    // elements behind. These states are therefore applied while the window is built.
    let before_first_frame = matches!(
        state,
        State::MainRowSelected
            | State::SearchFocusedEmpty
            | State::SearchTypingSt
            | State::SearchError
    );
    let window = cx.update(|cx| {
        cx.open_window(window_options(bounds), move |window, cx| {
            gpui_component::init(cx);
            let scope = RegionScope::all(&regions);
            let main =
                cx.new(|cx| MainWindow::new(fleet_for_window.clone(), regions, scope, window, cx));
            if before_first_frame {
                apply_render_state(
                    state,
                    &fleet_for_window,
                    &main,
                    DEFAULT_ROW_MENU_ANCHOR,
                    window,
                    cx,
                );
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

    if !before_first_frame {
        cx.update(|window, app| {
            apply_render_state(state, &fleet, &main, DEFAULT_ROW_MENU_ANCHOR, window, app)
        });
    }
    cx.run_until_parked();

    (Some(fleet), Some(main), cx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_record::recorded;

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

    /// One key element per state that `apply_render_state` must produce.
    #[gpui::test]
    fn every_state_shows_its_key_element(cx: &mut TestAppContext) {
        let key_elements: [(State, &str); 27] = [
            (State::MainIdle, "main.root"),
            (State::MainRowSelected, "main.inspector"),
            (State::MainEmpty, "main.center-state"),
            (State::MainFailure, "main.center-state"),
            (State::MainPartial, "bar.failure"),
            (State::MainLarge, "main.root"),
            (State::SearchFocusedEmpty, "search.recent"),
            (State::SearchTypingSt, "search.suggestions"),
            (State::SearchError, "search.error"),
            (State::OverlayProfile, "main.overlay.profile"),
            (State::OverlayRegion, "main.overlay.region"),
            (State::OverlayPalette, "main.overlay.palette"),
            (State::OverlayFailure, "main.overlay.failure"),
            (State::OverlayRowMenu, "main.overlay.rowmenu"),
            (State::AboutLight, "about.title"),
            (State::AboutDark, "about.title"),
            (State::AboutRefusedPlugin, "about.title"),
            (State::AboutLongPluginList, "about.title"),
            (State::AboutLongPluginName, "about.title"),
            (State::PluginsLight, "plugins.list"),
            (State::PluginsDark, "plugins.list"),
            (State::PluginsInstalled, "plugins.detail"),
            (State::PluginsRefused, "plugins.detail"),
            (State::PluginsEmptySearch, "plugins.empty"),
            (State::PluginsAvailableEmpty, "plugins.empty"),
            (State::PluginsRestart, "plugins.restart"),
            (State::PluginsNarrow, "plugins.list"),
        ];
        assert_eq!(
            key_elements.len() + 1,
            State::ALL.len(),
            "Welcome has no main window"
        );
        for (state, selector) in key_elements {
            let (_fleet, _main, window_cx) = build(state, 1280.0, 760.0, cx);
            assert!(
                window_cx.debug_bounds(selector).is_some(),
                "{state:?}: {selector}"
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
    fn load_failure_shows_the_red_status_segment(cx: &mut TestAppContext) {
        let (_fleet, _main, cx) = build(State::MainFailure, 1280.0, 760.0, cx);

        let status = cx
            .update(|_, app| recorded(app, "bar.status"))
            .expect("bar.status is recorded");
        assert_eq!(status.text.as_deref(), Some("Couldn\u{2019}t load"));
        assert_eq!(status.fg, Some("err"));
        assert!(cx.debug_bounds("bar.status").is_some());
    }

    #[gpui::test]
    fn idle_status_is_neutral_ready(cx: &mut TestAppContext) {
        let (_fleet, _main, cx) = build(State::MainIdle, 1280.0, 760.0, cx);

        let status = cx
            .update(|_, app| recorded(app, "bar.status"))
            .expect("bar.status is recorded");
        assert_eq!(status.text.as_deref(), Some("Ready"));
        assert_eq!(status.fg, Some("dim"));
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
        assert_eq!(
            fleet.read_with(cx, |fleet, _| fleet.query().to_string()),
            "st"
        );
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
