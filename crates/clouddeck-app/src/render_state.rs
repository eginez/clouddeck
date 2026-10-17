//! Named UI states shared by the layout checks and the `--render-state` launch flag.
//!
//! `apply_render_state` drives a loaded main window into a [`State`] through the same
//! `MainWindow` methods the keyboard shortcuts and clicks reach, so a test window and the
//! real app can never drift apart.

use gpui::{App, Entity, Pixels, Point, Window, point, px};

use crate::fleet_state::FleetState;
use crate::main_window::{MainWindow, Overlay};
use crate::shell::DemoScenario;

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
    /// Profile picker opened over the idle main window.
    OverlayProfile,
    /// Region picker opened over the idle main window.
    OverlayRegion,
    /// Command palette opened over the idle main window.
    OverlayPalette,
    /// Failure popover opened from the partial-failure bar.
    OverlayFailure,
    /// Row context menu opened on the first row.
    OverlayRowMenu,
    /// First-run welcome window listing three profiles.
    Welcome,
}

impl State {
    pub(crate) const ALL: [State; 15] = [
        State::MainIdle,
        State::MainRowSelected,
        State::MainEmpty,
        State::MainFailure,
        State::MainPartial,
        State::MainLarge,
        State::SearchFocusedEmpty,
        State::SearchTypingSt,
        State::SearchError,
        State::OverlayProfile,
        State::OverlayRegion,
        State::OverlayPalette,
        State::OverlayFailure,
        State::OverlayRowMenu,
        State::Welcome,
    ];

    /// The variant name, as accepted by `--render-state`.
    pub(crate) fn name(self) -> &'static str {
        match self {
            State::MainIdle => "MainIdle",
            State::MainRowSelected => "MainRowSelected",
            State::MainEmpty => "MainEmpty",
            State::MainFailure => "MainFailure",
            State::MainPartial => "MainPartial",
            State::MainLarge => "MainLarge",
            State::SearchFocusedEmpty => "SearchFocusedEmpty",
            State::SearchTypingSt => "SearchTypingSt",
            State::SearchError => "SearchError",
            State::OverlayProfile => "OverlayProfile",
            State::OverlayRegion => "OverlayRegion",
            State::OverlayPalette => "OverlayPalette",
            State::OverlayFailure => "OverlayFailure",
            State::OverlayRowMenu => "OverlayRowMenu",
            State::Welcome => "Welcome",
        }
    }

    pub(crate) fn from_name(name: &str) -> Option<State> {
        State::ALL.into_iter().find(|state| state.name() == name)
    }

    /// Comma-separated list of every valid name, for usage messages.
    pub(crate) fn names() -> String {
        State::ALL.map(State::name).join(", ")
    }

    /// The demo scenario whose data this state needs.
    pub(crate) fn demo_scenario(self) -> DemoScenario {
        match self {
            State::MainEmpty => DemoScenario::Empty,
            State::MainFailure => DemoScenario::Failure,
            State::MainPartial | State::OverlayFailure => DemoScenario::Partial,
            State::MainLarge => DemoScenario::Large,
            _ => DemoScenario::Happy,
        }
    }
}

/// Recent searches listed by [`State::SearchFocusedEmpty`].
const RECENT_SEARCHES: [&str; 4] = [
    "state:stopped age:>30d",
    "owner:esteban",
    "has:public-ip type:t3.*",
    "billing prod",
];

/// Where the row menu opens when the caller has no measured row position.
pub(crate) const DEFAULT_ROW_MENU_ANCHOR: Point<Pixels> = point(px(420.0), px(190.0));

/// Whether the fleet has finished its first load (success or error).
pub(crate) fn fleet_loaded(fleet: &FleetState) -> bool {
    !fleet.view_model.is_loading()
        && (fleet.view_model.last_success().is_some() || fleet.view_model.error().is_some())
}

/// Drives a loaded main window into `state`. `row_menu_anchor` is where the row menu opens
/// for [`State::OverlayRowMenu`]. The welcome state has no main window and is a no-op.
pub(crate) fn apply_render_state(
    state: State,
    fleet: &Entity<FleetState>,
    main: &Entity<MainWindow>,
    row_menu_anchor: Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let first_row_key = |cx: &App| {
        fleet
            .read(cx)
            .visible_rows()
            .first()
            .expect("fleet has a visible row")
            .id()
    };
    match state {
        State::MainRowSelected => {
            let key = first_row_key(cx);
            fleet.update(cx, |fleet, cx| fleet.select(Some(key), cx));
        }
        State::SearchFocusedEmpty => {
            fleet.update(cx, |fleet, cx| {
                fleet.recent_searches = RECENT_SEARCHES.map(String::from).to_vec();
                cx.notify();
            });
            focus_search(main, "", fleet, window, cx);
        }
        State::SearchTypingSt => focus_search(main, "st", fleet, window, cx),
        State::SearchError => focus_search(main, "colour:x", fleet, window, cx),
        State::OverlayProfile => open(main, Overlay::Profile, window, cx),
        State::OverlayRegion => open(main, Overlay::Region, window, cx),
        State::OverlayPalette => open(main, Overlay::Palette, window, cx),
        State::OverlayFailure => open(main, Overlay::Failure, window, cx),
        State::OverlayRowMenu => {
            let key = first_row_key(cx);
            main.update(cx, |main, cx| {
                main.open_row_menu(key, row_menu_anchor, window, cx)
            });
        }
        State::MainIdle
        | State::MainEmpty
        | State::MainFailure
        | State::MainPartial
        | State::MainLarge
        | State::Welcome => {}
    }
    // Focus and overlays settle on the next frame; make sure one is drawn.
    window.refresh();
}

fn open(main: &Entity<MainWindow>, overlay: Overlay, window: &mut Window, cx: &mut App) {
    main.update(cx, |main, cx| main.open_overlay(overlay, window, cx));
}

/// Focuses the search field and sets its text to `text`.
fn focus_search(
    main: &Entity<MainWindow>,
    text: &str,
    fleet: &Entity<FleetState>,
    window: &mut Window,
    cx: &mut App,
) {
    let search = main.read(cx).search_field();
    search.update(cx, |search, cx| search.focus(window, cx));
    if !text.is_empty() {
        fleet.update(cx, |fleet, cx| fleet.set_query(text, cx));
    }
}
