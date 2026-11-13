use crate::app_providers;
use crate::connect_window::{ConnectEvent, open_connect_window};
use crate::render_state::{DEFAULT_ROW_MENU_ANCHOR, State, apply_render_state, fleet_loaded};
use crate::{
    fleet_state::FleetState,
    main_window::{
        MainWindow, MainWindowEvent, ensure_region_dimension, region_values, set_region_values,
    },
    settings_window::{SettingsEvent, SettingsWindow, open_settings_window},
    theme::SurgicalTheme,
};
use clouddeck_core::{
    ALL_PROVIDERS, Call, Column, ConnectionId, ContextModel, DemoFleet, FLEET_REGIONS,
    InstanceListViewModel, ListView, LoadHost, PROVIDER_SINGLE_COLUMN_KEY, ProfileInput, Provider,
    ProviderEntry, REGION, Settings, UiState, ValueId, WindowFrame, WorkspaceContext,
    all_providers_view, demo_profiles, demo_region_names, provider_signed_in, settings_path,
    short_name, switch_active_provider, workspace_provider_for,
};
use gpui::{
    AnyWindowHandle, App, Bounds, Context, Entity, IntoElement, KeyBinding, ParentElement, Render,
    Styled, Subscription, Task, Window, WindowHandle, actions, div, point, prelude::*, px, size,
};
use gpui_component::Root;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};

mod construct;
mod helpers;
mod launch;
mod persist;
mod providers;
mod render;
#[cfg(test)]
mod tests;
mod window;

use self::helpers::*;
pub(crate) use self::launch::launch_options;
pub(crate) use self::window::{open_main_window, open_main_window_with};

actions!(clouddeck, [Refresh]);

const UI_STATE_SAVE_DEBOUNCE: Duration = Duration::from_secs(1);
/// Scope timeout handed to the AWS plugin; its own `request_timeout` setting takes precedence.
const SCOPE_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub fn app_key_bindings() -> [KeyBinding; 2] {
    [
        KeyBinding::new("secondary-q", crate::Quit, None),
        KeyBinding::new("secondary-r", Refresh, None),
    ]
}

pub struct AppShell {
    fleet: Entity<FleetState>,
    main_window: Entity<MainWindow>,
    window_handle: AnyWindowHandle,
    context: WorkspaceContext,
    context_model: ContextModel,
    demo: bool,
    demo_scenario: DemoFleet,
    message: Option<String>,
    ui_state_path: Option<PathBuf>,
    ui_state_saved: UiState,
    ui_window_frame: Option<WindowFrame>,
    ui_save_task: Option<Task<()>>,
    // Held only to keep these observers alive; dropping them unsubscribes.
    _window_bounds: Subscription,
    _quit_observer: Subscription,
    settings_subscription: Option<Subscription>,
    /// Region selection written to the UI state; `None` when nothing is remembered,
    /// an empty list when every region is selected.
    remembered_regions: Option<Vec<String>>,
    request_timeout: Duration,
    /// Test-only: an optional provider injected into `apply_context`
    /// so that a test can deterministically hold a picker-driven context
    /// switch load open (via a gated provider) and observe the
    /// `is_loading` / `Reloading` transition window that
    /// a static provider resolves too fast to reliably catch.
    #[cfg(test)]
    test_service_override: Option<std::sync::Arc<dyn Provider>>,
}
