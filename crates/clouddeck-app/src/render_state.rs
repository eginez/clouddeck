//! Named UI states shared by the layout checks and the `--render-state` launch flag.
//!
//! `apply_render_state` drives a loaded main window into a [`State`] through the same
//! `MainWindow` methods the keyboard shortcuts and clicks reach, so a test window and the
//! real app can never drift apart.

use gpui::{App, Entity, Pixels, Point, Window, point, px};

use crate::fleet_state::FleetState;
use crate::main_window::{MainWindow, Overlay};
use clouddeck_core::{Column, DemoFleet, GroupBy};

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
    /// The same failed load as `MainFailure`, in the dark palette.
    MainFailureDark,
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
    /// Column chooser opened over the idle main window, in the light palette.
    OverlayColumnMenu,
    /// Column chooser opened over the idle main window, in the dark palette.
    OverlayColumnMenuDark,
    /// All-providers list of AWS and Google Cloud rows, with the Provider column shown.
    MainAllProviders,
    /// The same All-providers list as `MainAllProviders`, in the dark palette.
    MainAllProvidersDark,
    /// The All-providers list with the Provider column hidden from the column chooser.
    MainAllProvidersNoColumn,
    /// The same hidden Provider column as `MainAllProvidersNoColumn`, in the dark palette.
    MainAllProvidersNoColumnDark,
    /// The All-providers list where Google Cloud failed to load and AWS rows still show.
    MainAllProvidersFailedGroup,
    /// The same failed Google Cloud group as `MainAllProvidersFailedGroup`, in the dark palette.
    MainAllProvidersFailedGroupDark,
    /// The All-providers list grouped by provider, with an AWS header and a Google Cloud header.
    MainAllProvidersGrouped,
    /// The same provider grouping as `MainAllProvidersGrouped`, in the dark palette.
    MainAllProvidersGroupedDark,
    /// The All-providers list at a narrow window, where the provider button and columns compete for width.
    MainAllProvidersNarrow,
    /// The All-providers list at the most compact window the layout supports.
    MainAllProvidersCompact,
    /// Google Cloud alone, as the single active provider with no Provider column.
    MainGcpActive,
    /// Google Cloud alone while signed out, so the sign-in button shows in the bar.
    MainGcpSignIn,
    /// The Google Cloud table as `MainGcpActive`, in the dark palette.
    MainGcpActiveDark,
    /// Region picker over a Google Cloud region tree, with zones indented under their regions.
    OverlayDimensionTree,
    /// Region picker over a single-select region dimension, with no checkboxes.
    OverlayDimensionSingle,
    /// Onboarding window with AWS selected and profiles listed, in the light palette.
    OnboardingReadyLight,
    /// The same onboarding window as `OnboardingReadyLight`, in the dark palette.
    OnboardingReadyDark,
    /// `OnboardingReadyLight` at the smallest width the onboarding window is laid out for.
    OnboardingReadyNarrow,
    /// Onboarding window still looking for connections.
    OnboardingDetecting,
    /// Onboarding window whose provider found no connections.
    OnboardingEmpty,
    /// Onboarding window whose provider failed with a recovery hint.
    OnboardingError,
    /// `OnboardingError` at the narrow width the onboarding window is laid out for.
    OnboardingErrorNarrow,
    /// Onboarding window after a connection was chosen to open.
    OnboardingOpening,
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
    ProviderFolderEmptyLight,
    ProviderFolderEmptyDark,
    ProviderFolderSetLight,
    ProviderFolderSetDark,
    ProviderFolderMissingLight,
    ProviderFolderMissingDark,
}

impl State {
    pub(crate) const ALL: [State; 72] = [
        State::MainIdle,
        State::MainRowSelected,
        State::MainEmpty,
        State::MainFailure,
        State::MainFailureDark,
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
        State::OverlayColumnMenu,
        State::OverlayColumnMenuDark,
        State::MainAllProviders,
        State::MainAllProvidersDark,
        State::MainAllProvidersNoColumn,
        State::MainAllProvidersNoColumnDark,
        State::MainAllProvidersFailedGroup,
        State::MainAllProvidersFailedGroupDark,
        State::MainAllProvidersGrouped,
        State::MainAllProvidersGroupedDark,
        State::MainAllProvidersNarrow,
        State::MainAllProvidersCompact,
        State::MainGcpActive,
        State::MainGcpSignIn,
        State::MainGcpActiveDark,
        State::OverlayDimensionTree,
        State::OverlayDimensionSingle,
        State::OnboardingReadyLight,
        State::OnboardingReadyDark,
        State::OnboardingReadyNarrow,
        State::OnboardingDetecting,
        State::OnboardingEmpty,
        State::OnboardingError,
        State::OnboardingErrorNarrow,
        State::OnboardingOpening,
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
        State::ProviderFolderEmptyLight,
        State::ProviderFolderEmptyDark,
        State::ProviderFolderSetLight,
        State::ProviderFolderSetDark,
        State::ProviderFolderMissingLight,
        State::ProviderFolderMissingDark,
    ];

    /// The variant name, as accepted by `--render-state`.
    pub(crate) fn name(self) -> &'static str {
        match self {
            State::MainIdle => "MainIdle",
            State::MainRowSelected => "MainRowSelected",
            State::MainEmpty => "MainEmpty",
            State::MainFailure => "MainFailure",
            State::MainFailureDark => "MainFailureDark",
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
            State::OverlayColumnMenu => "OverlayColumnMenu",
            State::OverlayColumnMenuDark => "OverlayColumnMenuDark",
            State::MainAllProviders => "MainAllProviders",
            State::MainAllProvidersDark => "MainAllProvidersDark",
            State::MainAllProvidersNoColumn => "MainAllProvidersNoColumn",
            State::MainAllProvidersNoColumnDark => "MainAllProvidersNoColumnDark",
            State::MainAllProvidersFailedGroup => "MainAllProvidersFailedGroup",
            State::MainAllProvidersFailedGroupDark => "MainAllProvidersFailedGroupDark",
            State::MainAllProvidersGrouped => "MainAllProvidersGrouped",
            State::MainAllProvidersGroupedDark => "MainAllProvidersGroupedDark",
            State::MainAllProvidersNarrow => "MainAllProvidersNarrow",
            State::MainAllProvidersCompact => "MainAllProvidersCompact",
            State::MainGcpActive => "MainGcpActive",
            State::MainGcpSignIn => "MainGcpSignIn",
            State::MainGcpActiveDark => "MainGcpActiveDark",
            State::OverlayDimensionTree => "OverlayDimensionTree",
            State::OverlayDimensionSingle => "OverlayDimensionSingle",
            State::OnboardingReadyLight => "OnboardingReadyLight",
            State::OnboardingReadyDark => "OnboardingReadyDark",
            State::OnboardingReadyNarrow => "OnboardingReadyNarrow",
            State::OnboardingDetecting => "OnboardingDetecting",
            State::OnboardingEmpty => "OnboardingEmpty",
            State::OnboardingError => "OnboardingError",
            State::OnboardingErrorNarrow => "OnboardingErrorNarrow",
            State::OnboardingOpening => "OnboardingOpening",
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
            State::ProviderFolderEmptyLight => "ProviderFolderEmptyLight",
            State::ProviderFolderEmptyDark => "ProviderFolderEmptyDark",
            State::ProviderFolderSetLight => "ProviderFolderSetLight",
            State::ProviderFolderSetDark => "ProviderFolderSetDark",
            State::ProviderFolderMissingLight => "ProviderFolderMissingLight",
            State::ProviderFolderMissingDark => "ProviderFolderMissingDark",
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
            State::MainStateGroupedDark
                | State::MainStateGroupedCollapsedDark
                | State::OverlayColumnMenuDark
                | State::MainAllProvidersDark
                | State::MainAllProvidersNoColumnDark
                | State::MainAllProvidersFailedGroupDark
                | State::MainAllProvidersGroupedDark
                | State::MainFailureDark
                | State::MainGcpActiveDark
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
                | State::ProviderFolderEmptyLight
                | State::ProviderFolderEmptyDark
                | State::ProviderFolderSetLight
                | State::ProviderFolderSetDark
                | State::ProviderFolderMissingLight
                | State::ProviderFolderMissingDark
        )
    }

    /// Whether the state is the onboarding window rather than a main-window state.
    #[cfg(test)]
    pub(crate) fn is_onboarding(self) -> bool {
        matches!(
            self,
            State::OnboardingReadyLight
                | State::OnboardingReadyDark
                | State::OnboardingReadyNarrow
                | State::OnboardingDetecting
                | State::OnboardingEmpty
                | State::OnboardingError
                | State::OnboardingErrorNarrow
                | State::OnboardingOpening
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
            State::MainFailure | State::MainFailureDark => DemoFleet::Failure,
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
/// for [`State::OverlayRowMenu`]. States for other windows have no main window and are a no-op.
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
        State::OverlayColumnMenu | State::OverlayColumnMenuDark => {
            fleet.update(cx, |fleet, cx| fleet.set_column_menu_open(true, cx));
        }
        State::MainAllProvidersNoColumn | State::MainAllProvidersNoColumnDark => {
            fleet.update(cx, |fleet, cx| fleet.toggle_column(Column::Provider, cx));
        }
        State::MainAllProvidersFailedGroup
        | State::MainAllProvidersFailedGroupDark
        | State::MainAllProvidersGrouped
        | State::MainAllProvidersGroupedDark => {
            fleet.update(cx, |fleet, cx| fleet.set_group_by(GroupBy::Provider, cx));
        }
        State::OverlayDimensionTree | State::OverlayDimensionSingle => {
            open(main, Overlay::Region, window, cx)
        }
        State::MainIdle
        | State::MainEmpty
        | State::MainFailure
        | State::MainFailureDark
        | State::MainPartial
        | State::MainLarge
        | State::MainAllProviders
        | State::MainAllProvidersDark
        | State::MainAllProvidersNarrow
        | State::MainAllProvidersCompact
        | State::MainGcpActive
        | State::MainGcpSignIn
        | State::MainGcpActiveDark
        | State::OnboardingReadyLight
        | State::OnboardingReadyDark
        | State::OnboardingReadyNarrow
        | State::OnboardingDetecting
        | State::OnboardingEmpty
        | State::OnboardingError
        | State::OnboardingErrorNarrow
        | State::OnboardingOpening
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
        | State::ProviderSettingsSearch
        | State::ProviderFolderEmptyLight
        | State::ProviderFolderEmptyDark
        | State::ProviderFolderSetLight
        | State::ProviderFolderSetDark
        | State::ProviderFolderMissingLight
        | State::ProviderFolderMissingDark => {}
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
