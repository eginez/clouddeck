//! Named UI states shared by the layout checks and the `--render-state` launch flag.
//!
//! `apply_render_state` drives a loaded main window into a [`State`] through the same
//! `MainWindow` methods the keyboard shortcuts and clicks reach, so a test window and the
//! real app can never drift apart.

use gpui::{App, Entity, Pixels, Point, Window, point, px};

use crate::fleet_state::FleetState;
use crate::main_window::{MainWindow, Overlay};
use clouddeck_core::{DemoFleet, GroupBy};

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
    /// Table grouped by state with every group expanded, in the light palette.
    MainStateGrouped,
    /// Table grouped by state with every group expanded, in the dark palette.
    MainStateGroupedDark,
    /// Table grouped by state with the Running group collapsed, in the light palette.
    MainStateGroupedCollapsed,
    /// Table grouped by state with the Running group collapsed, in the dark palette.
    MainStateGroupedCollapsedDark,
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
    /// About window in the light palette, with the demo plus two bundled or installed plugins.
    AboutLight,
    /// About window in the dark palette, with the same plugins as `AboutLight`.
    AboutDark,
    /// About window with one plugin refused by the registry.
    AboutRefusedPlugin,
    /// About window with enough plugins that the list scrolls.
    AboutLongPluginList,
    /// About window with one plugin whose name is too long for its column.
    AboutLongPluginName,
    /// Plugins window in the light palette with bundled aws, gcp and demo plugins installed.
    PluginsLight,
    /// Plugins window in the dark palette with the same plugins as `PluginsLight`.
    PluginsDark,
    /// Plugins window on the Installed tab with a detail panel open.
    PluginsInstalled,
    /// Plugins window with one plugin refused by the registry, selected.
    PluginsRefused,
    /// Plugins window with a search that matches nothing.
    PluginsEmptySearch,
    /// Plugins window on the Available tab, which has no index yet.
    PluginsAvailableEmpty,
    /// Plugins window showing the restart banner after a plugin changed.
    PluginsRestart,
    /// Plugins window at the narrowest supported width.
    PluginsNarrow,
    /// Settings window at its default size in the light palette.
    SettingsLight,
    /// Settings window at its default size in the dark palette.
    SettingsDark,
    /// Settings window at a width where the footer text has to be truncated.
    SettingsNarrow,
    /// Settings window at the narrow width in the dark palette.
    SettingsNarrowDark,
    /// Settings window on an extension's category with one setting of each kind, in the light palette.
    ProviderSettingsLight,
    /// The same extension category as `ProviderSettingsLight`, in the dark palette.
    ProviderSettingsDark,
    /// The extension category with a text setting changed, so its marker and Reset show.
    ProviderSettingsChanged,
    /// The extension category with a number outside its range, so the row shows an error.
    ProviderSettingsInvalid,
    /// Settings window with a search that matches an extension's setting.
    ProviderSettingsSearch,
}

impl State {
    pub(crate) const ALL: [State; 41] = [
        State::MainIdle,
        State::MainRowSelected,
        State::MainEmpty,
        State::MainFailure,
        State::MainPartial,
        State::MainLarge,
        State::MainStateGrouped,
        State::MainStateGroupedDark,
        State::MainStateGroupedCollapsed,
        State::MainStateGroupedCollapsedDark,
        State::SearchFocusedEmpty,
        State::SearchTypingSt,
        State::SearchError,
        State::OverlayProfile,
        State::OverlayRegion,
        State::OverlayPalette,
        State::OverlayFailure,
        State::OverlayRowMenu,
        State::Welcome,
        State::AboutLight,
        State::AboutDark,
        State::AboutRefusedPlugin,
        State::AboutLongPluginList,
        State::AboutLongPluginName,
        State::PluginsLight,
        State::PluginsDark,
        State::PluginsInstalled,
        State::PluginsRefused,
        State::PluginsEmptySearch,
        State::PluginsAvailableEmpty,
        State::PluginsRestart,
        State::PluginsNarrow,
        State::SettingsLight,
        State::SettingsDark,
        State::SettingsNarrow,
        State::SettingsNarrowDark,
        State::ProviderSettingsLight,
        State::ProviderSettingsDark,
        State::ProviderSettingsChanged,
        State::ProviderSettingsInvalid,
        State::ProviderSettingsSearch,
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
            State::MainStateGrouped => "MainStateGrouped",
            State::MainStateGroupedDark => "MainStateGroupedDark",
            State::MainStateGroupedCollapsed => "MainStateGroupedCollapsed",
            State::MainStateGroupedCollapsedDark => "MainStateGroupedCollapsedDark",
            State::SearchFocusedEmpty => "SearchFocusedEmpty",
            State::SearchTypingSt => "SearchTypingSt",
            State::SearchError => "SearchError",
            State::OverlayProfile => "OverlayProfile",
            State::OverlayRegion => "OverlayRegion",
            State::OverlayPalette => "OverlayPalette",
            State::OverlayFailure => "OverlayFailure",
            State::OverlayRowMenu => "OverlayRowMenu",
            State::Welcome => "Welcome",
            State::AboutLight => "AboutLight",
            State::AboutDark => "AboutDark",
            State::AboutRefusedPlugin => "AboutRefusedPlugin",
            State::AboutLongPluginList => "AboutLongPluginList",
            State::AboutLongPluginName => "AboutLongPluginName",
            State::PluginsLight => "PluginsLight",
            State::PluginsDark => "PluginsDark",
            State::PluginsInstalled => "PluginsInstalled",
            State::PluginsRefused => "PluginsRefused",
            State::PluginsEmptySearch => "PluginsEmptySearch",
            State::PluginsAvailableEmpty => "PluginsAvailableEmpty",
            State::PluginsRestart => "PluginsRestart",
            State::PluginsNarrow => "PluginsNarrow",
            State::SettingsLight => "SettingsLight",
            State::SettingsDark => "SettingsDark",
            State::SettingsNarrow => "SettingsNarrow",
            State::SettingsNarrowDark => "SettingsNarrowDark",
            State::ProviderSettingsLight => "ProviderSettingsLight",
            State::ProviderSettingsDark => "ProviderSettingsDark",
            State::ProviderSettingsChanged => "ProviderSettingsChanged",
            State::ProviderSettingsInvalid => "ProviderSettingsInvalid",
            State::ProviderSettingsSearch => "ProviderSettingsSearch",
        }
    }

    /// Whether the state groups the table by state, with or without a collapsed group.
    #[cfg(test)]
    pub(crate) fn is_grouped(self) -> bool {
        matches!(
            self,
            State::MainStateGrouped
                | State::MainStateGroupedDark
                | State::MainStateGroupedCollapsed
                | State::MainStateGroupedCollapsedDark
        )
    }

    /// Whether the state renders the main window in the dark palette.
    #[cfg(test)]
    pub(crate) fn is_dark_main(self) -> bool {
        matches!(
            self,
            State::MainStateGroupedDark | State::MainStateGroupedCollapsedDark
        )
    }

    /// Whether the state is the Plugins window rather than a main-window state.
    pub(crate) fn is_plugins(self) -> bool {
        matches!(
            self,
            State::PluginsLight
                | State::PluginsDark
                | State::PluginsInstalled
                | State::PluginsRefused
                | State::PluginsEmptySearch
                | State::PluginsAvailableEmpty
                | State::PluginsRestart
                | State::PluginsNarrow
        )
    }

    #[cfg(test)]
    pub(crate) fn is_settings(self) -> bool {
        matches!(
            self,
            State::SettingsLight
                | State::SettingsDark
                | State::SettingsNarrow
                | State::SettingsNarrowDark
                | State::ProviderSettingsLight
                | State::ProviderSettingsDark
                | State::ProviderSettingsChanged
                | State::ProviderSettingsInvalid
                | State::ProviderSettingsSearch
        )
    }

    /// Whether the state is the About window rather than a main-window state.
    pub(crate) fn is_about(self) -> bool {
        matches!(
            self,
            State::AboutLight
                | State::AboutDark
                | State::AboutRefusedPlugin
                | State::AboutLongPluginList
                | State::AboutLongPluginName
        )
    }

    pub(crate) fn from_name(name: &str) -> Option<State> {
        State::ALL.into_iter().find(|state| state.name() == name)
    }

    /// Comma-separated list of every valid name, for usage messages.
    pub(crate) fn names() -> String {
        State::ALL.map(State::name).join(", ")
    }

    /// The demo scenario whose data this state needs.
    pub(crate) fn demo_scenario(self) -> DemoFleet {
        match self {
            State::MainEmpty => DemoFleet::Empty,
            State::MainFailure => DemoFleet::Failure,
            State::MainPartial | State::OverlayFailure => DemoFleet::Partial,
            State::MainLarge => DemoFleet::Large,
            _ => DemoFleet::Happy,
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
            .id
            .clone()
    };
    match state {
        State::MainRowSelected => {
            let key = first_row_key(cx);
            fleet.update(cx, |fleet, cx| fleet.select(Some(key), cx));
        }
        State::MainStateGrouped | State::MainStateGroupedDark => {
            fleet.update(cx, |fleet, cx| fleet.set_group_by(GroupBy::State, cx));
        }
        State::MainStateGroupedCollapsed | State::MainStateGroupedCollapsedDark => {
            fleet.update(cx, |fleet, cx| {
                fleet.set_group_by(GroupBy::State, cx);
                fleet.toggle_group("Running", cx);
            });
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
        | State::Welcome
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
        | State::PluginsNarrow
        | State::SettingsLight
        | State::SettingsDark
        | State::SettingsNarrow
        | State::SettingsNarrowDark
        | State::ProviderSettingsLight
        | State::ProviderSettingsDark
        | State::ProviderSettingsChanged
        | State::ProviderSettingsInvalid
        | State::ProviderSettingsSearch => {}
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
