//! Main window layout: toolbar, sidebar, table or centered state, inspector, status bar (design/images/01, 04, 41-43).

use std::time::Duration;

use clouddeck_core::{
    AddressKind, CenterContext, CenterInput, CenterState, CenterText, CloudDeckError,
    CommandContext, CommandId, ComputeServicePartialFailure, ContextModel, DimValue, Dimension,
    DimensionId, EscapeAction, EscapeContext, Instance, ListResult, MAIN_WINDOW_MIN_WIDTH,
    OPEN_IN_CONSOLE_FLASH, ProfileInput, ProviderMenuRow, REGION, Settings, Theme, ValueId,
    center_state, center_text, clock_text, console_url_for, copied_flash, escape_action,
    filter_command_query, known_tag_keys, layout_rules, native_id, region_failure_reason,
    settings_path, should_auto_refresh, should_refresh_on_focus, stale_count, updated_text,
};
use gpui::{
    AnyElement, AnyView, App, Bounds, ClickEvent, ClipboardItem, Context, Corner, Div, Entity,
    EventEmitter, FocusHandle, Focusable, Global, IntoElement, KeyBinding, MouseDownEvent, Pixels,
    Point, Render, SharedString, Subscription, Task, TitlebarOptions, Window, WindowAppearance,
    WindowBounds, WindowOptions, actions, anchored, deferred, div, hsla, point, prelude::*, px,
    relative, size,
};

use crate::app_menus;
use crate::center_state_view::{CenterEvent, CenterStateView};
use crate::command_palette::{CommandPalette, PaletteEvent};
use crate::context_bar::{ContextBar, ContextBarEvent};
use crate::dimension_picker_view::{DimensionPicker, DimensionPickerEvent};
use crate::failure_popover::{FailurePopover, FailurePopoverEvent, RegionFailure};
use crate::fleet_state::FleetState;
use crate::inspector_pane::InspectorPane;
use crate::instance_table::{InstanceTable, InstanceTableEvent, table_key_bindings};
use crate::profile_picker_view::{ProfilePicker, ProfilePickerEvent};
use crate::provider_picker_view::{ProviderPicker, ProviderPickerEvent};
use crate::row_context_menu::{RowContextMenu, RowMenuEvent};
use crate::search_field::{SearchField, search_key_bindings};
use crate::sidebar::Sidebar;
use crate::tokens::{
    ActivePalette, FONT_SIZE, INSPECTOR_WIDTH, Palette, SIDEBAR_WIDTH, STATUS_BAR_HEIGHT, UI_FONT,
    is_dark,
};
use crate::toolbar::{Toolbar, ToolbarEvent};

mod commands;
mod construct;
mod overlay;
mod panes;
mod refresh;
mod render;
mod sync;

actions!(main_window, [EscapePressed]);

const OVERLAY_MAX_FRACTION: f32 = 0.92;
const SCRIM_ALPHA: f32 = 0.2;
const MIN_WINDOW_HEIGHT: f32 = 400.0;
const OVERLAY_ABOVE_STATUS_BAR: f32 = STATUS_BAR_HEIGHT + 4.0;
const REGION_OVERLAY_LEFT: f32 = 150.0;
const PALETTE_TOP: f32 = 80.0;
const PALETTE_WIDTH: f32 = 560.0;
const OVERLAY_EDGE_INSET: f32 = 8.0;
const AUTO_REFRESH_TICK: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq)]
pub enum Overlay {
    Profile,
    Provider,
    Region,
    Palette,
    RowMenu {
        key: String,
        position: Point<Pixels>,
    },
    Failure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MainWindowEvent {
    PickProfile(String),
    PickProvider(String),
    SignIn(String),
    RegionsChanged(Vec<String>),
    Retry(Option<String>),
    OpenSettings,
}

struct MenusInstalled;

impl Global for MenusInstalled {}

pub struct MainWindow {
    pub fleet: Entity<FleetState>,
    pub available_regions: Vec<String>,
    pub failures: Vec<ComputeServicePartialFailure>,
    pub profiles: Vec<ProfileInput>,
    pub provider_short_name: String,
    pub connection_label: String,
    pub sign_in: bool,
    pub provider_menu: Vec<ProviderMenuRow>,
    pub context_model: ContextModel,
    pub overlay: Option<Overlay>,
    toolbar: Entity<Toolbar>,
    sidebar: Entity<Sidebar>,
    table: Entity<InstanceTable>,
    inspector: Entity<InspectorPane>,
    context_bar: Entity<ContextBar>,
    center: Entity<CenterStateView>,
    search: Entity<SearchField>,
    focus_handle: FocusHandle,
    overlay_view: Option<AnyView>,
    overlay_subscription: Option<Subscription>,
    region_dirty: bool,
    /// Whether the window was narrow on the last render, to reset panes only when it crosses.
    last_narrow: Option<bool>,
    /// The last load error, kept through the next in-flight load so the status bar doesn't flicker.
    last_load_error: Option<CloudDeckError>,
    auto_refresh: bool,
    refresh_interval_minutes: u32,
    refresh_on_focus: bool,
    window_active: bool,
    theme: Theme,
    _appearance_observer: Subscription,
    _auto_refresh_loop: Task<()>,
    /// Elements drawn by the last render, so tests can see removals that `debug_bounds` keeps.
    #[cfg(test)]
    rendered: RenderedPanes,
    #[cfg(test)]
    rendered_overlay: Option<&'static str>,
}

impl EventEmitter<MainWindowEvent> for MainWindow {}

#[cfg(test)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct RenderedPanes {
    sidebar: bool,
    inspector: bool,
    scrim: bool,
    center_state: bool,
}

fn apply_appearance(theme: Theme, window: &mut Window, cx: &mut App) {
    let dark = is_dark(theme, system_is_dark(window));
    cx.set_global(ActivePalette(Palette::for_dark(dark)));
    gpui_component::Theme::change(theme_mode(dark), Some(window), cx);
    cx.refresh_windows();
}

fn system_is_dark(window: &Window) -> bool {
    matches!(
        window.appearance(),
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    )
}

fn install_menus(cx: &mut App) {
    app_menus::install(cx);
    cx.bind_keys(table_key_bindings());
    cx.bind_keys(search_key_bindings());
    cx.bind_keys([KeyBinding::new("escape", EscapePressed, Some("MainWindow"))]);
    cx.bind_keys(crate::linux_titlebar::key_bindings());
    cx.set_global(MenusInstalled);
}

fn theme_mode(dark: bool) -> gpui_component::ThemeMode {
    if dark {
        gpui_component::ThemeMode::Dark
    } else {
        gpui_component::ThemeMode::Light
    }
}

pub(crate) fn region_dimension(available: &[String]) -> Dimension {
    Dimension {
        id: DimensionId::from(REGION),
        label: "Region".to_string(),
        plural_label: None,
        multi: true,
        optional: false,
        values: available
            .iter()
            .map(|region| DimValue {
                id: ValueId::from(region.as_str()),
                label: region.clone(),
                parent: None,
                default: false,
            })
            .collect(),
    }
}

pub(crate) fn ensure_region_dimension(model: &mut ContextModel, available: &[String]) {
    let shares_app_regions = model.active_provider() == "demo";
    let has_region = model
        .dimensions()
        .iter()
        .any(|dim| dim.id.as_str() == REGION);
    if has_region && !shares_app_regions {
        return;
    }
    let mut dimensions: Vec<Dimension> = model
        .dimensions()
        .iter()
        .filter(|dim| dim.id.as_str() != REGION)
        .cloned()
        .collect();
    dimensions.push(region_dimension(available));
    model.set_dimensions(dimensions);
}

pub(crate) fn region_values(model: &ContextModel) -> Vec<String> {
    model
        .selection()
        .values(REGION)
        .iter()
        .map(|value| value.as_str().to_string())
        .collect()
}

pub(crate) fn set_region_values(model: &mut ContextModel, regions: &[String]) {
    model.set_values(
        REGION,
        regions
            .iter()
            .map(|region| ValueId::from(region.as_str()))
            .collect(),
    );
}

fn region_model(available: &[String], instances: &[Instance]) -> ContextModel {
    let mut model = ContextModel::new(Vec::new(), None, "");
    model.set_dimensions(vec![region_dimension(available)]);
    model.set_list_result(
        &ListResult {
            resources: instances.to_vec(),
            failures: Vec::new(),
        },
        |instance| {
            instance
                .region
                .iter()
                .map(|region| ValueId::from(region.as_str()))
                .collect()
        },
    );
    model
}

fn overlay_panel(selector: &'static str, view: AnyView) -> Div {
    div()
        .debug_selector(move || selector.into())
        .occlude()
        .child(view)
}

pub const APP_ID: &str = "clouddeck";

pub fn window_options(bounds: Bounds<Pixels>) -> WindowOptions {
    WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some(SharedString::from("CloudDeck")),
            appears_transparent: true,
            traffic_light_position: Some(point(px(14.0), px(15.0))),
        }),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(px(MAIN_WINDOW_MIN_WIDTH), px(MIN_WINDOW_HEIGHT))),
        app_id: Some(APP_ID.to_string()),
        ..Default::default()
    }
}

fn local_offset_minutes() -> i32 {
    chrono::Local::now().offset().local_minus_utc() / 60
}

#[cfg(test)]
mod tests;
