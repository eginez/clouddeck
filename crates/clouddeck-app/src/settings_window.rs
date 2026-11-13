//! Settings window (design/images/50-59).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use clouddeck_core::extension_host::{
    ProviderRegistry, SettingsCategory, search_provider_settings,
};
use clouddeck_core::{
    DefaultSort, Density, KEYMAP, LaunchBehavior, Platform, RECENT_SEARCH_LIMITS,
    REFRESH_INTERVALS, SETTINGS_CATEGORIES, SettingDef, SettingId, SettingKind, SettingValue,
    Settings, SettingsSearchResults, Theme, UiState, aws_files_setting, changed_categories,
    context_path, search_settings, settings_no_match_text, settings_path, settings_results_title,
    settings_search_breadcrumb, shortcut_label, ui_state_path,
};
use gpui::{
    AnyElement, AnyView, App, Bounds, ClickEvent, Context, Entity, EventEmitter, Global,
    IntoElement, ParentElement, PathPromptOptions, Render, SharedString, Styled, Subscription,
    TitlebarOptions, Window, WindowBounds, WindowHandle, WindowOptions, div, img, point,
    prelude::*, px, size, white,
};
use gpui_component::IndexPath;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::select::{Select, SelectEvent, SelectItem, SelectState};
use gpui_component::tooltip::Tooltip;
use gpui_component::{Root, Sizable};

use crate::app_menus;
use crate::icons::AppIcon;
use crate::setting_rows::{
    ProviderChoice, RowParts, button, choice_field, choice_state, display_text, folder_field,
    folder_is_missing, highlighted, link, setting_row, switch, text_field, text_state,
};
use crate::tokens::{Palette, palette};

mod actions;
mod construct;
mod helpers;
mod pane;
mod provider;
mod provider_pane;
mod render;
mod sections;
use self::helpers::*;

const SAVE_DEBOUNCE: Duration = Duration::from_millis(300);
const NOTE_DURATION: Duration = Duration::from_secs(2);
pub(crate) const WINDOW_SIZE: (f32, f32) = (820.0, 560.0);

const FOOTER_HINT: &str =
    "Changes apply immediately. A blue bar marks values that differ from the default.";

const CATEGORY_ICONS: [AppIcon; 9] = [
    AppIcon::PanelLeft,
    AppIcon::Server,
    AppIcon::Filter,
    AppIcon::Search,
    AppIcon::Key,
    AppIcon::Cloud,
    AppIcon::Ellipsis,
    AppIcon::Plug,
    AppIcon::CircleAlert,
];
const PROVIDER_CATEGORY_ICON: AppIcon = AppIcon::Plug;

fn category_icon(index: usize) -> AppIcon {
    CATEGORY_ICONS
        .get(index)
        .copied()
        .unwrap_or(PROVIDER_CATEGORY_ICON)
}

type FieldKey = (String, String);

#[derive(Debug, Clone, PartialEq)]
pub enum SettingsEvent {
    Changed(Settings),
    ProviderSettingChanged {
        extension: String,
        key: Option<String>,
    },
    RecentSearchesCleared,
    RememberedScopeForgotten,
    ConnectAgain(String),
}

pub struct SettingsWindow {
    pub settings: Settings,
    pub category: &'static str,
    providers: Arc<ProviderRegistry>,
    provider_categories: Vec<SettingsCategory>,
    provider: Option<String>,
    provider_inputs: Vec<(FieldKey, Entity<InputState>)>,
    provider_selects: Vec<(FieldKey, Entity<SelectState<Vec<ProviderChoice>>>)>,
    setting_errors: BTreeMap<FieldKey, String>,
    search: Entity<InputState>,
    selects: Vec<(SettingId, Entity<SelectState<Vec<Choice>>>)>,
    _subscriptions: Vec<Subscription>,
    path: PathBuf,
    unsaved: bool,
    save_generation: u64,
    note: Option<Note>,
    note_generation: u64,
    /// Settings drawn by the last render, since `debug_bounds` keeps removed elements.
    #[cfg(test)]
    rendered: Vec<SettingId>,
    #[cfg(test)]
    rendered_changes: Vec<SettingId>,
    #[cfg(test)]
    rendered_dots: Vec<usize>,
    #[cfg(test)]
    rendered_search: bool,
    #[cfg(test)]
    rendered_categories: Vec<usize>,
}

impl EventEmitter<SettingsEvent> for SettingsWindow {}

struct OpenSettingsWindow(WindowHandle<Root>);

impl Global for OpenSettingsWindow {}

pub fn open_settings_window(
    cx: &mut App,
    settings: Settings,
    providers: Arc<ProviderRegistry>,
) -> Option<WindowHandle<Root>> {
    if let Some(handle) = cx.try_global::<OpenSettingsWindow>().map(|open| open.0)
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return Some(handle);
    }
    let handle = open_window_for(cx, settings, providers, WINDOW_SIZE)?;
    cx.set_global(OpenSettingsWindow(handle));
    Some(handle)
}

/// Always opens a new window, bypassing the single-instance check.
pub(crate) fn open_window_for(
    cx: &mut App,
    settings: Settings,
    providers: Arc<ProviderRegistry>,
    window_size: (f32, f32),
) -> Option<WindowHandle<Root>> {
    let bounds = Bounds::centered(None, size(px(window_size.0), px(window_size.1)), cx);
    cx.open_window(
        WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some(SharedString::from("Settings")),
                appears_transparent: true,
                traffic_light_position: Some(point(px(14.0), px(15.0))),
            }),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            app_id: Some(crate::main_window::APP_ID.to_string()),
            ..Default::default()
        },
        move |window, cx| {
            let view = cx.new(|cx| SettingsWindow::new(settings, providers, window, cx));
            let weak = view.downgrade();
            window.on_window_should_close(cx, move |_, cx| {
                let _ = weak.update(cx, |this, _| this.flush_pending_save());
                true
            });
            cx.new(|cx| Root::new(view, window, cx))
        },
    )
    .ok()
}

fn edit_ui_state_file(path: &Path, edit: impl FnOnce(&mut UiState)) -> std::io::Result<()> {
    let mut state = UiState::try_load(path)?;
    edit(&mut state);
    state.save(path)
}

/// The window's root view, for layout checks that drive it without clicks.
#[cfg(test)]
pub(crate) fn window_view(
    window: WindowHandle<Root>,
    cx: &mut App,
) -> Option<Entity<SettingsWindow>> {
    window
        .update(cx, |root, _, _| {
            root.view().clone().downcast::<SettingsWindow>().ok()
        })
        .ok()
        .flatten()
}

/// Two extensions with settings and one without, discovered from `dir` as enabled external extensions.
#[cfg(test)]
pub(crate) fn provider_fixture(dir: &Path) -> ProviderRegistry {
    use std::fs;

    const SAMPLE: &str = r#"id = "sample"
name = "Sample"
version = "0.1.0"
schema_version = 1
api_version = 7

[providers.sample]
name = "Sample"

[[settings]]
key = "region_label"
label = "Region label"
description = "Shown beside each region"
group = "Location"
kind = "text"
default = "us-east-1"

[[settings]]
key = "config_path"
label = "Config path"
group = "Location"
kind = "path"
default = "~/.config/sample"

[[settings]]
key = "verbose"
label = "Verbose logging"
group = "Logging"
kind = "bool"
default = false

[[settings]]
key = "retries"
label = "Retries"
group = "Logging"
kind = "number"
default = 3
min = 0
max = 10

[[settings]]
key = "timeout_secs"
label = "Request timeout"
group = "Logging"
kind = "duration_secs"
default = 30
min = 5
max = 120

[[settings]]
key = "mode"
label = "Mode"
kind = "choice"
default = "fast"
options = [{ value = "fast", label = "Fast" }, { value = "safe", label = "Safe" }]

[[settings]]
key = "token"
label = "API token"
kind = "secret"
"#;
    const TOY: &str = r#"id = "toy"
name = "Toy"
version = "0.1.0"
schema_version = 1
api_version = 7

[providers.toy]
name = "Toy"

[[settings]]
key = "display_name"
label = "Display name"
kind = "text"
default = "Default"

[[settings]]
key = "config_dir"
label = "Config folder"
kind = "folder"
"#;
    const PLAIN: &str = r#"id = "plain"
name = "Plain"
version = "0.1.0"
schema_version = 1
api_version = 7

[providers.plain]
name = "Plain"
"#;
    let root = dir.join("extensions");
    let state_root = dir.join("extension-state");
    fs::create_dir_all(&state_root).unwrap();
    for (id, manifest) in [("sample", SAMPLE), ("toy", TOY), ("plain", PLAIN)] {
        let folder = root.join(id);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("extension.toml"), manifest).unwrap();
        fs::write(state_root.join(format!("{id}.json")), r#"{"enabled":true}"#).unwrap();
    }
    let mut registry = ProviderRegistry::default()
        .with_settings_file(dir.join("settings.json"))
        .with_external(true);
    registry.discover(&root, &state_root).unwrap();
    registry
}

#[cfg(test)]
mod tests;
