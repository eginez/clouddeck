use crate::app_providers;
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
    ALL_PROVIDERS, Call, Column, ContextModel, DemoFleet, FLEET_REGIONS, InstanceListViewModel,
    ListView, LoadHost, PROVIDER_SINGLE_COLUMN_KEY, ProfileInput, Provider, ProviderEntry, REGION,
    Settings, UiState, ValueId, WindowFrame, WorkspaceContext, all_providers_view, demo_profiles,
    demo_region_names, settings_path, short_name, switch_active_provider, workspace_provider_for,
};
use gpui::{
    AnyWindowHandle, App, Bounds, Context, Entity, IntoElement, KeyBinding, ParentElement, Render,
    Styled, Subscription, Task, Window, WindowHandle, actions, div, point, prelude::*, px, size,
};
use gpui_component::Root;
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

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
    sign_ins_in_flight: Rc<RefCell<HashSet<String>>>,
    demo: bool,
    demo_scenario: DemoFleet,
    wizard: Option<Entity<crate::account_window::AccountWindowView>>,
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

/// What the command line asked for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LaunchOptions {
    pub demo: bool,
    pub scenario: DemoFleet,
    /// Content size of the main window, from `--size WxH`.
    pub size: Option<(f32, f32)>,
    /// State to open the main window in, from `--render-state <Name>`.
    pub state: Option<State>,
    /// Print the version and exit, from --version or -V.
    pub print_version: bool,
}

/// Parses `WxH` (positive whole numbers) into a window content size.
fn parse_size(text: &str) -> Option<(f32, f32)> {
    let (width, height) = text.split_once('x')?;
    let width: u16 = width.parse().ok().filter(|w| *w > 0)?;
    let height: u16 = height.parse().ok().filter(|h| *h > 0)?;
    Some((f32::from(width), f32::from(height)))
}

/// Parses launch arguments (without the program name). `--render-state` implies `--demo`
/// and the scenario that state needs unless `--demo-scenario` is also given.
pub(crate) fn parse_launch_options(
    args: impl IntoIterator<Item = String>,
) -> Result<LaunchOptions, String> {
    let mut demo = false;
    let mut explicit_scenario = None;
    let mut size = None;
    let mut state = None;
    let mut print_version = false;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--demo" => demo = true,
            "--version" | "-V" => print_version = true,
            "--demo-scenario" => {
                explicit_scenario = Some(match args.next().as_deref() {
                    Some("empty") => DemoFleet::Empty,
                    Some("failure") => DemoFleet::Failure,
                    Some("partial") => DemoFleet::Partial,
                    Some("large") => DemoFleet::Large,
                    Some("huge") => DemoFleet::Huge,
                    _ => DemoFleet::Happy,
                });
                demo = true;
            }
            "--size" => {
                let value = args.next().unwrap_or_default();
                size = Some(parse_size(&value).ok_or_else(|| {
                    format!("invalid --size '{value}': expected WxH, e.g. 520x760")
                })?);
            }
            "--render-state" => {
                let value = args.next().unwrap_or_default();
                state = Some(State::from_name(&value).ok_or_else(|| {
                    format!(
                        "unknown --render-state '{value}'; valid names: {}",
                        State::names()
                    )
                })?);
                demo = true;
            }
            _ => {}
        }
    }
    let scenario = explicit_scenario
        .or_else(|| state.map(State::demo_scenario))
        .unwrap_or(DemoFleet::Happy);
    Ok(LaunchOptions {
        demo,
        scenario,
        size,
        state,
        print_version,
    })
}

const USAGE: &str = "usage: clouddeck [--demo] [--demo-scenario <name>] [--size WxH] [--render-state <Name>] [--version]";

/// Launch options from the process arguments; exits with status 2 on a usage error
/// and status 0 after printing the version.
pub(crate) fn launch_options() -> LaunchOptions {
    let options = parse_launch_options(std::env::args().skip(1)).unwrap_or_else(|message| {
        eprintln!("clouddeck: {message}");
        eprintln!("{USAGE}");
        std::process::exit(2)
    });
    if options.print_version {
        println!("clouddeck {}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }
    options
}

fn restored_regions(saved: &[String], available: &[String]) -> Vec<String> {
    let kept: Vec<String> = available
        .iter()
        .filter(|region| saved.contains(region))
        .cloned()
        .collect();
    if kept.is_empty() {
        available.to_vec()
    } else {
        kept
    }
}

fn setup_mode(profile: &str) -> crate::account_window::AccountWindowMode {
    if profile.is_empty() {
        crate::account_window::AccountWindowMode::SetupRequired
    } else {
        crate::account_window::AccountWindowMode::ManageContext
    }
}

fn accepts_provider(id: &str, known: bool) -> Option<()> {
    if id == ALL_PROVIDERS || known {
        Some(())
    } else {
        None
    }
}

fn keeps_switch(switched: bool, needs_sign_in: bool) -> Option<()> {
    if switched || needs_sign_in {
        Some(())
    } else {
        None
    }
}

fn record_save_failure(message: &mut Option<String>, saved: Option<std::io::Result<()>>) {
    if let Some(Err(error)) = saved {
        *message = Some(format!("Could not remember this provider: {error}"));
    }
}

fn reload_target(
    shows_all: bool,
    needs_sign_in: bool,
    current: WorkspaceContext,
    first_profile: Option<String>,
) -> Option<WorkspaceContext> {
    if shows_all {
        return Some(current);
    }
    match first_profile {
        Some(profile) if !needs_sign_in => Some(WorkspaceContext { profile }),
        _ => None,
    }
}

fn should_remember_context(demo: bool, usable: bool) -> bool {
    !demo && usable
}

fn restore_ui_state(
    path: PathBuf,
    loaded: std::io::Result<UiState>,
) -> (UiState, Option<PathBuf>, Option<String>) {
    loaded.map_or_else(
        |error| {
            eprintln!("Could not read UI state; leaving it unchanged: {error}");
            (
                UiState::default(),
                None,
                Some(format!(
                    "Could not read UI state, so it will not be saved this session: {error}"
                )),
            )
        },
        |state| (state, Some(path), None),
    )
}

/// Tests never pass `--demo`, so `cargo test` runs always see real mode here.
#[cfg(test)]
fn is_real_mode() -> bool {
    !std::env::args_os().any(|arg| arg == "--demo")
}

pub(crate) fn open_main_window(
    cx: &mut App,
    demo: bool,
    demo_scenario: DemoFleet,
) -> Option<WindowHandle<Root>> {
    open_main_window_with(cx, demo, demo_scenario, None, None)
}

/// Opens the main window at `size` (content size) and, if given, drives it into `state`
/// once the fleet has loaded, then forces a redraw so the first frame needs no input.
pub(crate) fn open_main_window_with(
    cx: &mut App,
    demo: bool,
    demo_scenario: DemoFleet,
    size: Option<(f32, f32)>,
    state: Option<State>,
) -> Option<WindowHandle<Root>> {
    let handle = open_window_sized(cx, demo, demo_scenario, size)?;
    if let Some(state) = state {
        apply_state_when_loaded(handle, state, cx);
        let _ = handle.update(cx, |_, window, _| window.refresh());
    }
    Some(handle)
}

fn apply_state_when_loaded(handle: WindowHandle<Root>, state: State, cx: &mut App) {
    let parts = handle
        .update(cx, |root, _, cx| {
            let shell = root.view().clone().downcast::<AppShell>().ok()?;
            let shell = shell.read(cx);
            Some((shell.fleet.clone(), shell.main_window.clone()))
        })
        .ok()
        .flatten();
    let Some((fleet, main)) = parts else { return };
    let applied = std::rc::Rc::new(std::cell::Cell::new(false));
    cx.observe(&fleet.clone(), move |observed, cx| {
        if applied.get() || !fleet_loaded(observed.read(cx)) {
            return;
        }
        applied.set(true);
        let _ = handle.update(cx, |_, window, cx| {
            apply_render_state(state, &observed, &main, DEFAULT_ROW_MENU_ANCHOR, window, cx);
            main.update(cx, |_, cx| cx.notify());
            window.refresh();
        });
    })
    .detach();
}

fn open_window_sized(
    cx: &mut App,
    demo: bool,
    demo_scenario: DemoFleet,
    size_override: Option<(f32, f32)>,
) -> Option<WindowHandle<Root>> {
    let restored = (!demo)
        .then(|| UiState::load(&clouddeck_core::ui_state_path()).window)
        .flatten()
        .map(|frame| {
            Bounds::new(
                point(px(frame.x), px(frame.y)),
                size(px(frame.width), px(frame.height)),
            )
        })
        .filter(|bounds| {
            cx.displays()
                .iter()
                .any(|display| display.bounds().intersects(bounds))
        });
    let bounds = match size_override {
        Some((width, height)) => Bounds::centered(None, size(px(width), px(height)), cx),
        None => restored.unwrap_or_else(|| Bounds::centered(None, size(px(1280.0), px(800.0)), cx)),
    };
    cx.open_window(crate::main_window::window_options(bounds), |window, cx| {
        let shell = cx.new(|cx| AppShell::build_with_scenario(window, demo, demo_scenario, cx));
        cx.new(|cx| gpui_component::Root::new(shell, window, cx))
    })
    .ok()
}

pub(crate) fn open_main_window_with_setup(cx: &mut App) -> Option<WindowHandle<Root>> {
    let handle = open_main_window(cx, false, DemoFleet::Happy)?;
    let shell = handle
        .update(cx, |root, _, _| {
            root.view().clone().downcast::<AppShell>().ok()
        })
        .ok()
        .flatten()?;
    shell.update(cx, |shell, cx| shell.open_setup(cx));
    Some(handle)
}

impl AppShell {
    #[cfg(test)]
    fn new_demo(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build_with_scenario(window, true, DemoFleet::Happy, cx)
    }

    #[cfg(test)]
    fn new_demo_empty(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build_with_scenario(window, true, DemoFleet::Empty, cx)
    }

    #[cfg(test)]
    fn new_demo_failure(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build_with_scenario(window, true, DemoFleet::Failure, cx)
    }

    #[cfg(test)]
    fn new_demo_large(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build_with_scenario(window, true, DemoFleet::Large, cx)
    }

    #[cfg(test)]
    fn new_demo_partial(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build_with_scenario(window, true, DemoFleet::Partial, cx)
    }

    fn profiles(
        demo: bool,
        demo_scenario: DemoFleet,
    ) -> Result<Vec<clouddeck_core::AwsProfile>, std::io::Error> {
        if demo {
            Ok(demo_profiles(demo_scenario))
        } else {
            clouddeck_core::load_profiles_for(clouddeck_core::aws_config_setting().as_deref())
        }
    }

    fn regions(demo: bool, demo_scenario: DemoFleet) -> Vec<String> {
        if !demo {
            return clouddeck_core::REGIONS
                .iter()
                .map(|region| (*region).into())
                .collect();
        }
        let partial_regions: Vec<String> = if demo_scenario == DemoFleet::Partial {
            FLEET_REGIONS
                .iter()
                .map(|region| (*region).into())
                .collect()
        } else {
            Vec::new()
        };
        let mut regions = vec!["fleet".to_string()];
        for region in demo_region_names(demo_scenario)
            .into_iter()
            .chain(partial_regions)
        {
            if !regions.iter().any(|known| known == &region) {
                regions.push(region);
            }
        }
        regions
    }

    fn available_regions(demo: bool, demo_scenario: DemoFleet) -> Vec<String> {
        let listed = Self::regions(demo, demo_scenario)
            .into_iter()
            .filter(|region| region != "fleet")
            .collect();
        Self::extend_demo_regions(listed, demo)
    }

    fn extend_demo_regions(mut regions: Vec<String>, demo: bool) -> Vec<String> {
        if demo {
            for region in clouddeck_core::REGIONS.iter().copied() {
                if region != "fleet" && !regions.iter().any(|known| known == region) {
                    regions.push(region.to_string());
                }
            }
        }
        regions
    }

    fn fleet_regions_for(model: &ContextModel) -> Vec<String> {
        if model.is_all(REGION) {
            return FLEET_REGIONS
                .iter()
                .map(|region| (*region).into())
                .collect();
        }
        region_values(model)
    }

    fn remembered_regions_for(model: &ContextModel) -> Vec<String> {
        if model.is_all(REGION) {
            return Vec::new();
        }
        region_values(model)
    }

    fn wizard_region(model: &ContextModel) -> String {
        match region_values(model).as_slice() {
            [only] if !model.is_all(REGION) => only.clone(),
            _ => "fleet".into(),
        }
    }

    fn apply_wizard_region(model: &mut ContextModel, region: &str, available: &[String]) {
        if region != "fleet" {
            set_region_values(model, &[region.to_string()]);
        } else if region_values(model).len() == 1 {
            set_region_values(model, available);
        }
    }

    fn window_frame(window: &Window) -> WindowFrame {
        let bounds = window.bounds();
        WindowFrame {
            x: f32::from(bounds.origin.x),
            y: f32::from(bounds.origin.y),
            width: f32::from(bounds.size.width),
            height: f32::from(bounds.size.height),
        }
    }

    #[cfg(test)]
    fn provider_for(
        providers: &[std::sync::Arc<dyn Provider>],
        context: &WorkspaceContext,
        demo: bool,
        demo_fleet: DemoFleet,
        fleet_regions: &[String],
        request_timeout: Duration,
    ) -> std::sync::Arc<dyn Provider> {
        Self::provider_for_id(
            "aws",
            providers,
            context,
            demo,
            demo_fleet,
            fleet_regions,
            request_timeout,
        )
    }

    fn provider_for_id(
        id: &str,
        providers: &[std::sync::Arc<dyn Provider>],
        context: &WorkspaceContext,
        demo: bool,
        demo_fleet: DemoFleet,
        fleet_regions: &[String],
        request_timeout: Duration,
    ) -> std::sync::Arc<dyn Provider> {
        workspace_provider_for(
            id,
            providers,
            &context.profile,
            fleet_regions,
            demo.then_some(demo_fleet),
            request_timeout,
        )
    }

    fn load_connections(model: &mut ContextModel, providers: &[std::sync::Arc<dyn Provider>]) {
        let active = model.active_provider().to_string();
        let Some(provider) = providers.iter().find(|p| p.info().id == active) else {
            return;
        };
        let call = Call {
            deadline: None,
            host: &LoadHost,
        };
        model.set_connections(provider.connections(&call));
        model.set_signed_in(&active, !model.needs_sign_in());
    }

    fn load_dimensions(model: &mut ContextModel, providers: &[std::sync::Arc<dyn Provider>]) {
        let active = model.active_provider().to_string();
        let Some(provider) = providers.iter().find(|p| p.info().id == active) else {
            return;
        };
        let Some(connection) = model.connection().cloned() else {
            return;
        };
        let call = Call {
            deadline: None,
            host: &LoadHost,
        };
        if let Ok(dimensions) = provider.dimensions(&call, &connection) {
            model.set_dimensions(dimensions);
        }
    }

    pub(crate) fn sync_provider_view(
        main_window: &Entity<MainWindow>,
        model: &ContextModel,
        cx: &mut App,
    ) {
        let provider_short_name = if model.shows_all_providers() {
            "All".to_string()
        } else {
            model
                .active_info()
                .map(short_name)
                .unwrap_or_else(|| "AWS".to_string())
        };
        let connection_label = model.connection_label().unwrap_or("Profile").to_string();
        let sign_in = model.needs_sign_in();
        let provider_menu = model.provider_menu();
        main_window.update(cx, |main_window, cx| {
            main_window.provider_short_name = provider_short_name;
            main_window.connection_label = connection_label;
            main_window.sign_in = sign_in;
            main_window.provider_menu = provider_menu;
            main_window.context_model = model.clone();
            cx.notify();
        });
    }

    fn build_with_scenario(
        window: &mut Window,
        demo: bool,
        demo_scenario: DemoFleet,
        cx: &mut Context<Self>,
    ) -> Self {
        let ui_state_path = (!demo).then(clouddeck_core::ui_state_path);
        Self::build(window, demo, demo_scenario, ui_state_path, cx)
    }

    fn build(
        window: &mut Window,
        demo: bool,
        demo_scenario: DemoFleet,
        ui_state_path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Self {
        let loaded = Self::profiles(demo, demo_scenario);
        let settings = Settings::load(&settings_path());
        Self::build_with_profiles(
            window,
            demo,
            demo_scenario,
            loaded,
            ui_state_path,
            settings,
            cx,
        )
    }

    fn build_with_profiles(
        window: &mut Window,
        demo: bool,
        demo_scenario: DemoFleet,
        loaded: Result<Vec<clouddeck_core::AwsProfile>, std::io::Error>,
        ui_state_path: Option<PathBuf>,
        settings: Settings,
        cx: &mut Context<Self>,
    ) -> Self {
        let message = loaded
            .as_ref()
            .err()
            .map(|e| format!("Could not read AWS profiles: {e}"));
        let profiles = loaded.unwrap_or_default();
        let saved = (!demo)
            .then(|| WorkspaceContext::load(&clouddeck_core::context_path()))
            .flatten();
        let ui_state_file_present = ui_state_path.as_ref().is_some_and(|path| path.exists());
        let (mut ui_state, ui_state_path, ui_state_message) = ui_state_path.map_or_else(
            || (UiState::default(), None, None),
            |path| {
                let loaded = UiState::try_load(&path);
                restore_ui_state(path, loaded)
            },
        );
        let ui_state_existed = ui_state_file_present && ui_state_path.is_some();
        let message = message.or(ui_state_message);
        let available = Self::available_regions(demo, demo_scenario);
        let context = saved
            .filter(|s| profiles.iter().any(|p| p.name == s.profile))
            .unwrap_or_else(|| WorkspaceContext {
                profile: profiles.first().map(|p| p.name.clone()).unwrap_or_default(),
            });
        let legacy_regions = ui_state.take_legacy_regions();
        let default_regions = if demo && demo_scenario == DemoFleet::Partial {
            available.clone()
        } else {
            let profile_region = profiles
                .iter()
                .find(|p| p.name == context.profile)
                .and_then(|p| p.region.clone())
                .unwrap_or_else(|| "us-east-1".into());
            if available.contains(&profile_region) {
                vec![profile_region]
            } else {
                available.clone()
            }
        };
        let saved_regions = match ui_state.dimension_selections.get(REGION) {
            Some(stored) => restored_regions(stored, &available),
            None => match legacy_regions {
                Some(legacy) => restored_regions(&legacy, &available),
                None if ui_state_existed => available.clone(),
                None => default_regions,
            },
        };
        let request_timeout = SCOPE_REQUEST_TIMEOUT;
        let providers = app_providers::providers(cx);
        let mut context_model = ContextModel::new(
            providers
                .iter()
                .filter(|provider| demo || provider.info().id != "demo")
                .map(|provider| ProviderEntry {
                    info: provider.info(),
                    signed_in: false,
                    bundled: true,
                })
                .collect(),
            settings.active_provider.as_deref(),
            "aws",
        );
        Self::load_connections(&mut context_model, &providers);
        Self::load_dimensions(&mut context_model, &providers);
        ensure_region_dimension(&mut context_model, &available);
        set_region_values(&mut context_model, &saved_regions);
        let fleet_regions = Self::fleet_regions_for(&context_model);
        let remembered_regions = Some(Self::remembered_regions_for(&context_model));
        let view: ListView = if context_model.shows_all_providers() {
            all_providers_view(&providers, demo.then_some(demo_scenario), request_timeout)
        } else {
            InstanceListViewModel::new(Self::provider_for_id(
                context_model.active_provider(),
                &providers,
                &context,
                demo,
                demo_scenario,
                &fleet_regions,
                request_timeout,
            ))
            .into()
        };
        let mut fleet_state = FleetState::new(view, context.profile.clone(), demo);
        fleet_state.pinned_searches = ui_state.pinned_searches.clone();
        fleet_state.recent_searches = ui_state.recent_searches.clone();
        fleet_state.recent_instances = ui_state.recent_instances.clone();
        fleet_state.sidebar_open = ui_state.sidebar_open;
        fleet_state.inspector_open = ui_state.inspector_open;
        fleet_state.hidden_columns = ui_state
            .hidden_columns
            .iter()
            .filter_map(|key| Column::from_key(key))
            .collect();
        fleet_state.single_provider_column_flipped = ui_state
            .hidden_columns
            .iter()
            .any(|key| key == PROVIDER_SINGLE_COLUMN_KEY);
        let fleet = cx.new(|_| fleet_state);
        let profile_inputs: Vec<ProfileInput> = profiles
            .iter()
            .map(|p| ProfileInput {
                name: p.name.clone(),
                account_id: None,
                region: p.region.clone(),
            })
            .collect();
        let main_window = Self::new_main_window(
            fleet.clone(),
            profile_inputs,
            demo,
            demo_scenario,
            window,
            cx,
        );
        Self::sync_provider_view(&main_window, &context_model, cx);
        let window_bounds = cx.observe_window_bounds(window, |this: &mut Self, window, cx| {
            this.ui_window_frame = Some(Self::window_frame(window));
            this.schedule_ui_state_save(cx);
        });
        let quit_observer = cx.on_app_quit(|this: &mut Self, cx| {
            this.save_ui_state(cx);
            std::future::ready(())
        });
        let weak = cx.weak_entity();
        window.on_window_should_close(cx, move |_, cx| {
            let _ = weak.update(cx, |this, cx| this.save_ui_state(cx));
            if cfg!(target_os = "linux") {
                app_providers::shutdown(cx);
            }
            true
        });
        fleet.update(cx, |fleet, cx| fleet.refresh(cx));
        Self {
            fleet,
            main_window,
            window_handle: window.window_handle(),
            context,
            context_model,
            sign_ins_in_flight: Rc::default(),
            demo,
            demo_scenario,
            message,
            ui_state_saved: ui_state.clone(),
            ui_window_frame: ui_state.window,
            ui_state_path,
            ui_save_task: None,
            _window_bounds: window_bounds,
            _quit_observer: quit_observer,
            settings_subscription: None,
            remembered_regions,
            request_timeout,
            wizard: None,
            #[cfg(test)]
            test_service_override: None,
        }
    }

    pub fn toggle_demo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let demo = !self.demo;
        let scenario = self.demo_scenario;
        self.save_ui_state(cx);
        *self = Self::build_with_scenario(window, demo, scenario, cx);
        cx.notify();
    }

    fn schedule_ui_state_save(&mut self, cx: &mut Context<Self>) {
        if self.ui_state_path.is_none() {
            return;
        }
        self.ui_save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(UI_STATE_SAVE_DEBOUNCE).await;
            let _ = this.update(cx, |this, cx| this.save_ui_state(cx));
        }));
    }

    fn save_ui_state(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.ui_state_path.clone() else {
            return;
        };
        let state = self.ui_state_snapshot(cx);
        if state == self.ui_state_saved {
            return;
        }
        match state.save(&path) {
            Ok(()) => self.ui_state_saved = state,
            Err(error) => {
                eprintln!("Could not save UI state: {error}");
                self.message = Some(format!("Could not save UI state: {error}"));
            }
        }
    }

    fn handle_settings_event(&mut self, event: &SettingsEvent, cx: &mut Context<Self>) {
        match event {
            SettingsEvent::Changed(settings) => {
                let main_window = self.main_window.clone();
                let _ = self.window_handle.update(cx, |_, window, cx| {
                    main_window.update(cx, |main_window, cx| {
                        main_window.apply_settings(settings, window, cx)
                    })
                });
            }
            SettingsEvent::RecentSearchesCleared => {
                self.ui_state_saved.recent_searches.clear();
                self.fleet.update(cx, |fleet, cx| {
                    fleet.recent_searches.clear();
                    cx.notify();
                });
            }
            SettingsEvent::RememberedScopeForgotten => {
                self.ui_state_saved.dimension_selections.clear();
                self.remembered_regions = None;
            }
        }
    }

    fn ui_state_snapshot(&self, cx: &App) -> UiState {
        let fleet = self.fleet.read(cx);
        let dimension_selections = match &self.remembered_regions {
            Some(regions) if !regions.is_empty() => {
                BTreeMap::from([(REGION.to_string(), regions.clone())])
            }
            _ => BTreeMap::new(),
        };
        UiState {
            dimension_selections,
            selected_regions: None,
            pinned_searches: fleet.pinned_searches.clone(),
            recent_searches: fleet.recent_searches.clone(),
            recent_instances: fleet.recent_instances.clone(),
            sidebar_open: fleet.sidebar_open,
            inspector_open: fleet.inspector_open,
            sidebar_width: self.ui_state_saved.sidebar_width,
            inspector_width: self.ui_state_saved.inspector_width,
            window: self.ui_window_frame.clone(),
            hidden_columns: fleet
                .hidden_columns
                .iter()
                .map(|column| column.key().to_string())
                .chain(
                    fleet
                        .single_provider_column_flipped
                        .then(|| PROVIDER_SINGLE_COLUMN_KEY.to_string()),
                )
                .collect(),
        }
        .normalized()
    }

    /// Test-only: install a compute service that will be used (instead of
    /// the scenario-derived one) by the *next* `apply_context` call, so a
    /// test can deterministically observe the loading window of a
    /// picker-driven context switch. Must be called before the trigger
    /// that causes `apply_context` to fire.
    #[cfg(test)]
    pub fn set_test_service_override(&mut self, provider: std::sync::Arc<dyn Provider>) {
        self.test_service_override = Some(provider);
    }

    fn new_main_window(
        fleet: Entity<FleetState>,
        profiles: Vec<ProfileInput>,
        demo: bool,
        demo_scenario: DemoFleet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<MainWindow> {
        let regions = Self::available_regions(demo, demo_scenario);
        let main_window = cx.new(|cx| MainWindow::new(fleet.clone(), regions, window, cx));
        main_window.update(cx, |main_window, _| main_window.profiles = profiles);
        let weak = main_window.downgrade();
        cx.observe(&fleet, move |_, observed, cx| {
            let failures = observed.read(cx).view_model.partial_failures().to_vec();
            let _ = weak.update(cx, |main_window, cx| {
                main_window.failures = failures;
                cx.notify();
            });
        })
        .detach();
        cx.observe(&fleet, |this: &mut Self, observed, cx| {
            if observed.entity_id() == this.fleet.entity_id() {
                let result = observed.read(cx).view_model.last_result().cloned();
                if let Some(result) = result {
                    this.context_model.set_list_result(&result, |instance| {
                        instance
                            .region
                            .iter()
                            .map(|region| ValueId::from(region.as_str()))
                            .collect()
                    });
                    Self::sync_provider_view(&this.main_window, &this.context_model, cx);
                }
            }
            this.schedule_ui_state_save(cx)
        })
        .detach();
        cx.subscribe_in(
            &main_window,
            window,
            |this: &mut Self, _, event: &MainWindowEvent, window, cx| match event {
                MainWindowEvent::PickProfile(name) => {
                    let next = WorkspaceContext {
                        profile: name.clone(),
                    };
                    if next != this.context {
                        this.apply_context(next, window, cx);
                    }
                }
                MainWindowEvent::PickProvider(id) => this.pick_provider(id, window, cx),
                MainWindowEvent::RegionsChanged(regions) => {
                    set_region_values(&mut this.context_model, regions);
                    let context = this.context.clone();
                    this.apply_context(context, window, cx);
                }
                MainWindowEvent::ConnectToAws => this.open_setup(cx),
                MainWindowEvent::SignIn(id) => this.sign_in(id, cx),
                MainWindowEvent::Retry(_) => this.refresh(cx),
                MainWindowEvent::OpenSettings => this.open_settings(cx),
            },
        )
        .detach();
        main_window
    }

    fn sign_in(&mut self, id: &str, cx: &mut Context<Self>) {
        if id == "aws" {
            self.open_setup(cx);
            return;
        }
        let providers = app_providers::providers(cx);
        let Some(provider) = providers
            .into_iter()
            .find(|provider| provider.info().id == id)
        else {
            self.fleet.update(cx, |fleet, cx| {
                fleet.set_flash(format!("{id} is not installed"), cx)
            });
            return;
        };
        let shell = cx.weak_entity();
        let window_handle = self.window_handle;
        let on_signed_in = move |cx: &mut gpui::App| {
            let _ = window_handle.update(cx, |_, window, cx| {
                let _ = shell.update(cx, |shell, cx| {
                    let providers = app_providers::providers(cx);
                    shell.reload_active_provider(&providers, window, cx);
                    shell.refresh(cx);
                });
            });
        };
        crate::provider_sign_in::begin_provider_sign_in(
            provider,
            self.fleet.clone(),
            self.sign_ins_in_flight.clone(),
            on_signed_in,
            cx,
        );
    }

    fn pick_provider(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.switch_provider_to(id, &settings_path(), window, cx);
    }

    fn switch_provider_to(
        &mut self,
        id: &str,
        settings_file: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        let providers = app_providers::providers(cx);
        let known = providers.iter().any(|provider| provider.info().id == id);
        accepts_provider(id, known)?;
        let mut settings = Settings::load(settings_file);
        let switched = switch_active_provider(&mut self.context_model, &mut settings, id);
        keeps_switch(switched, self.context_model.needs_sign_in())?;
        record_save_failure(
            &mut self.message,
            switched.then(|| settings.save(settings_file)),
        );
        self.reload_active_provider(&providers, window, cx);
        Some(())
    }

    fn reload_active_provider(
        &mut self,
        providers: &[std::sync::Arc<dyn Provider>],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let regions = region_values(&self.context_model);
        Self::load_connections(&mut self.context_model, providers);
        Self::load_dimensions(&mut self.context_model, providers);
        let available = Self::available_regions(self.demo, self.demo_scenario);
        ensure_region_dimension(&mut self.context_model, &available);
        set_region_values(&mut self.context_model, &regions);
        let first_profile = self
            .context_model
            .connections()
            .first()
            .map(|connection| connection.id.as_str().to_string());
        reload_target(
            self.context_model.shows_all_providers(),
            self.context_model.needs_sign_in(),
            self.context.clone(),
            first_profile,
        )
        .map(|context| self.apply_context(context, window, cx))
        .unwrap_or_else(|| Self::sync_provider_view(&self.main_window, &self.context_model, cx));
    }

    fn open_settings(&mut self, cx: &mut Context<Self>) {
        let providers = crate::app_providers::registry(cx);
        let Some(handle) = open_settings_window(cx, Settings::load(&settings_path()), providers)
        else {
            return;
        };
        let Ok(Some(view)) = handle.update(cx, |root, _, _| {
            root.view().clone().downcast::<SettingsWindow>().ok()
        }) else {
            return;
        };
        self.settings_subscription =
            Some(cx.subscribe(&view, |this, _, event: &SettingsEvent, cx| {
                this.handle_settings_event(event, cx)
            }));
    }

    fn apply_context(
        &mut self,
        context: WorkspaceContext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.context = context;
        let profiles = self.main_window.read(cx).profiles.clone();
        let fleet_regions = Self::fleet_regions_for(&self.context_model);
        self.remembered_regions = Some(Self::remembered_regions_for(&self.context_model));
        let providers = app_providers::providers(cx);
        let view = self.list_view(&providers, &fleet_regions);
        let profile = self.context.profile.clone();
        let demo = self.demo;
        let previous = self.fleet.read(cx);
        let (pinned, recent_searches, recent_instances, sidebar_open, inspector_open) = (
            previous.pinned_searches.clone(),
            previous.recent_searches.clone(),
            previous.recent_instances.clone(),
            previous.sidebar_open,
            previous.inspector_open,
        );
        let hidden_columns = previous.hidden_columns.clone();
        let single_provider_column_flipped = previous.single_provider_column_flipped;
        // A fresh fleet and window, not an in-place reset: the previous load
        // holds only a weak handle to the old fleet, so it cannot write into
        // the new context once that entity is dropped.
        let fleet = cx.new(|_| {
            let mut fleet = FleetState::new(view, profile, demo);
            fleet.pinned_searches = pinned;
            fleet.recent_searches = recent_searches;
            fleet.recent_instances = recent_instances;
            fleet.sidebar_open = sidebar_open;
            fleet.inspector_open = inspector_open;
            fleet.hidden_columns = hidden_columns;
            fleet.single_provider_column_flipped = single_provider_column_flipped;
            fleet
        });
        self.main_window = Self::new_main_window(
            fleet.clone(),
            profiles,
            demo,
            self.demo_scenario,
            window,
            cx,
        );
        Self::sync_provider_view(&self.main_window, &self.context_model, cx);
        self.fleet = fleet;
        self.fleet.update(cx, |fleet, cx| fleet.refresh(cx));
        should_remember_context(self.demo, self.context.is_usable()).then(|| {
            self.message = self
                .context
                .save(&clouddeck_core::context_path())
                .err()
                .map(|e| format!("Could not remember this context: {e}"))
        });
        self.schedule_ui_state_save(cx);
        cx.notify();
    }

    fn list_view(
        &self,
        providers: &[std::sync::Arc<dyn Provider>],
        fleet_regions: &[String],
    ) -> ListView {
        if self.context_model.shows_all_providers() {
            all_providers_view(
                providers,
                self.demo.then_some(self.demo_scenario),
                self.request_timeout,
            )
        } else {
            self.active_provider_view(providers, fleet_regions)
        }
    }

    fn active_provider_view(
        &self,
        providers: &[std::sync::Arc<dyn Provider>],
        fleet_regions: &[String],
    ) -> ListView {
        #[cfg(test)]
        let provider = self.test_service_override.clone().unwrap_or_else(|| {
            Self::provider_for_id(
                self.context_model.active_provider(),
                providers,
                &self.context,
                self.demo,
                self.demo_scenario,
                fleet_regions,
                self.request_timeout,
            )
        });
        #[cfg(not(test))]
        let provider = Self::provider_for_id(
            self.context_model.active_provider(),
            providers,
            &self.context,
            self.demo,
            self.demo_scenario,
            fleet_regions,
            self.request_timeout,
        );
        InstanceListViewModel::new(provider).into()
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.fleet.update(cx, |fleet, cx| fleet.refresh(cx));
    }

    fn open_setup(&mut self, cx: &mut Context<Self>) {
        use crate::account_window::AccountWindowView;
        let profiles = Self::profiles(self.demo, self.demo_scenario).unwrap_or_else(|error| {
            self.message = Some(format!("Could not read AWS profiles: {error}"));
            vec![]
        });
        let profile = self.context.profile.clone();
        let region = Self::wizard_region(&self.context_model);
        let mode = setup_mode(&profile);
        let demo = self.demo;
        let demo_scenario = self.demo_scenario;
        let shell = cx.weak_entity();
        let window_handle = self.window_handle;
        let bounds = gpui::Bounds::centered(None, gpui::size(px(1000.0), px(760.0)), cx);
        let result = cx.open_window(
            gpui::WindowOptions {
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("CloudDeck Setup".into()),
                    ..Default::default()
                }),
                window_bounds: Some(gpui::WindowBounds::Windowed(bounds)),
                app_id: Some(crate::main_window::APP_ID.to_string()),
                ..Default::default()
            },
            move |_, cx| {
                let wizard = cx.new(|_| AccountWindowView {
                    profiles: Some(profiles),
                    available_regions: Some(Self::regions(demo, demo_scenario)),
                    selected_profile: if profile.is_empty() {
                        None
                    } else {
                        Some(profile)
                    },
                    selected_region: region,
                    mode,
                    ..Default::default()
                });
                cx.observe(&wizard, move |wizard, cx| {
                    // Demo mode rebuilds the whole shell, so it needs the AppShell
                    // and its window, which `account_connect::drive` does not have.
                    if wizard.read(cx).demo_requested {
                        wizard.update(cx, |view, _cx| view.demo_requested = false);
                        let _ = window_handle.update(cx, |_, window, cx| {
                            let _ = shell.update(cx, |shell, cx| shell.toggle_demo(window, cx));
                        });
                        return;
                    }
                    // Drives the SDK-backed connection check (STS identity,
                    // EC2 access, browser SSO sign-in, rescan) behind the
                    // wizard's one-shot request flags. See
                    // `account_connect` for why this lives in its own
                    // module rather than inline here.
                    crate::account_connect::drive(&wizard, cx);
                    if wizard.read(cx).finished {
                        let next = WorkspaceContext {
                            profile: wizard.read(cx).selected_profile.clone().unwrap_or_default(),
                        };
                        let wizard_region = wizard.read(cx).selected_region.clone();
                        let _ = window_handle.update(cx, |_, window, cx| {
                            let _ = shell.update(cx, |shell, cx| {
                                let available =
                                    Self::available_regions(shell.demo, shell.demo_scenario);
                                let mut target_model = shell.context_model.clone();
                                Self::apply_wizard_region(
                                    &mut target_model,
                                    &wizard_region,
                                    &available,
                                );
                                let target_regions = region_values(&target_model);
                                let was_sign_in = shell.context_model.needs_sign_in();
                                if !was_sign_in
                                    && next == shell.context
                                    && target_regions == region_values(&shell.context_model)
                                {
                                    return;
                                }
                                let providers = app_providers::providers(cx);
                                Self::load_connections(&mut shell.context_model, &providers);
                                Self::load_dimensions(&mut shell.context_model, &providers);
                                ensure_region_dimension(&mut shell.context_model, &available);
                                set_region_values(&mut shell.context_model, &target_regions);
                                Self::sync_provider_view(
                                    &shell.main_window,
                                    &shell.context_model,
                                    cx,
                                );
                                shell.apply_context(next, window, cx);
                            });
                        });
                    }
                })
                .detach();
                wizard
            },
        );
        result
            .map(|handle| {
                self.wizard = handle.update(cx, |_, _, cx| cx.entity()).ok();
            })
            .unwrap_or_else(|error| {
                self.message = Some(format!("Could not open setup: {error}"));
            });
    }
}

impl Render for AppShell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .debug_selector(|| "app-shell".into())
            .flex()
            .flex_col()
            .size_full()
            .bg(SurgicalTheme::workspace_canvas())
            .children(self.message.clone().map(|m| {
                div()
                    .px(px(16.0))
                    .text_color(SurgicalTheme::warning())
                    .child(m)
            }))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .min_w(px(0.0))
                    .w_full()
                    .child(self.main_window.clone()),
            )
            .on_action(cx.listener(|this, _: &Refresh, _, cx| this.refresh(cx)))
    }
}

#[cfg(test)]
mod provider_switch_tests {
    use super::*;
    use gpui::{TestAppContext, VisualTestContext};

    fn add_rooted_shell(
        cx: &mut TestAppContext,
        build: impl FnOnce(&mut Window, &mut Context<AppShell>) -> AppShell + 'static,
    ) -> (Entity<AppShell>, &mut VisualTestContext) {
        let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
        let slot_for_window = slot.clone();
        let (_root, cx) = cx.add_window_view(move |window, cx| {
            gpui_component::init(cx);
            let shell = cx.new(|cx| build(window, cx));
            *slot_for_window.borrow_mut() = Some(shell.clone());
            gpui_component::Root::new(shell, window, cx)
        });
        let shell = slot
            .borrow()
            .as_ref()
            .expect("rooted shell entity should be created")
            .clone();
        (shell, cx)
    }

    fn compute_provider(id: &str) -> std::sync::Arc<dyn Provider> {
        std::sync::Arc::new(clouddeck_core::ComputeServiceProvider::new(
            id,
            vec![clouddeck_core::Connection {
                id: "default".into(),
                display_name: "default".to_string(),
            }],
            Vec::new(),
            |_, _| Box::new(clouddeck_core::MockComputeService::new(Vec::new(), None)),
        ))
    }

    struct SignedOut(std::sync::Arc<dyn Provider>);

    impl Provider for SignedOut {
        fn info(&self) -> clouddeck_core::ProviderInfo {
            self.0.info()
        }

        fn connections(
            &self,
            _cx: &clouddeck_core::Call<'_>,
        ) -> Result<Vec<clouddeck_core::Connection>, clouddeck_core::ProviderError> {
            Err(clouddeck_core::ProviderError::of(
                clouddeck_core::ErrorKind::Auth,
            ))
        }

        fn dimensions(
            &self,
            cx: &clouddeck_core::Call<'_>,
            connection: &clouddeck_core::ConnectionId,
        ) -> Result<Vec<clouddeck_core::Dimension>, clouddeck_core::ProviderError> {
            self.0.dimensions(cx, connection)
        }

        fn kinds(&self) -> Vec<clouddeck_core::KindInfo> {
            self.0.kinds()
        }

        fn list(
            &self,
            cx: &clouddeck_core::Call<'_>,
            connection: &clouddeck_core::ConnectionId,
            kind: &clouddeck_core::KindId,
            selection: &clouddeck_core::Selection,
        ) -> clouddeck_core::ListResult {
            self.0.list(cx, connection, kind, selection)
        }

        fn console_url(
            &self,
            connection: &clouddeck_core::ConnectionId,
            kind: &clouddeck_core::KindId,
            resource: &clouddeck_core::Instance,
        ) -> Option<String> {
            self.0.console_url(connection, kind, resource)
        }

        fn fields(&self, kind: &clouddeck_core::KindId) -> Vec<clouddeck_core::FieldDef> {
            self.0.fields(kind)
        }
    }

    fn install_providers(cx: &mut TestAppContext, providers: Vec<std::sync::Arc<dyn Provider>>) {
        cx.update(|cx| {
            let registry = clouddeck_core::extension_host::app_registry(
                None,
                Path::new("/nonexistent-clouddeck-extensions"),
                Path::new("/nonexistent-clouddeck-state"),
                providers,
            );
            cx.set_global(crate::app_providers::AppProviders::new(registry));
        });
    }

    fn install_registry(cx: &mut TestAppContext, gcp: std::sync::Arc<dyn Provider>) {
        install_providers(cx, vec![compute_provider("aws"), gcp]);
    }

    fn bare_ec2(region: &str, instance_id: &str) -> clouddeck_core::EC2Instance {
        clouddeck_core::EC2Instance {
            instance_id: instance_id.to_string(),
            region: Some(region.to_string()),
            instance_type: None,
            state: None,
            launch_time: None,
            public_ip_address: None,
            private_ip_address: None,
            vpc_id: None,
            subnet_id: None,
            security_groups: Vec::new(),
            tags: Vec::new(),
            availability_zone: None,
            image_id: None,
            key_name: None,
            architecture: None,
            state_transition_reason: None,
            iam_instance_profile_arn: None,
            private_dns_name: None,
            public_dns_name: None,
        }
    }

    fn recording_aws(
        regions: Vec<String>,
    ) -> (
        std::sync::Arc<dyn Provider>,
        std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    ) {
        let queried = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = queried.clone();
        let provider = clouddeck_core::ComputeServiceProvider::new(
            "aws",
            vec![clouddeck_core::Connection {
                id: "default".into(),
                display_name: "default".to_string(),
            }],
            regions,
            move |_, region| {
                sink.lock().unwrap().push(region.to_string());
                Box::new(clouddeck_core::MockComputeService::new(
                    vec![bare_ec2(region, &format!("i-{region}"))],
                    None,
                ))
            },
        );
        (std::sync::Arc::new(provider), queried)
    }

    fn sorted_queries(queried: &std::sync::Mutex<Vec<String>>) -> Vec<String> {
        let mut regions = queried.lock().unwrap().clone();
        regions.sort();
        regions.dedup();
        regions
    }

    fn region_dimension_values(
        shell: &Entity<AppShell>,
        cx: &mut VisualTestContext,
    ) -> Vec<String> {
        shell.read_with(cx, |shell, _| {
            shell
                .context_model
                .dimensions()
                .iter()
                .find(|dim| dim.id.as_str() == REGION)
                .map(|dim| {
                    dim.values
                        .iter()
                        .map(|v| v.id.as_str().to_string())
                        .collect()
                })
                .unwrap_or_default()
        })
    }

    fn aws_regions() -> Vec<String> {
        clouddeck_core::REGIONS
            .iter()
            .map(|region| (*region).to_string())
            .collect()
    }

    fn shell_with_ui_state(
        cx: &mut TestAppContext,
        aws: std::sync::Arc<dyn Provider>,
        ui_file: Option<std::path::PathBuf>,
    ) -> (Entity<AppShell>, &mut VisualTestContext) {
        install_providers(cx, vec![aws]);
        add_rooted_shell(cx, move |window, cx| {
            AppShell::build_with_profiles(
                window,
                false,
                DemoFleet::Happy,
                Ok(Vec::new()),
                ui_file,
                Settings::default(),
                cx,
            )
        })
    }

    fn shell_restored_from_ui_file<'a>(
        cx: &'a mut TestAppContext,
        name: &str,
        contents: &str,
    ) -> (Entity<AppShell>, &'a mut VisualTestContext) {
        let ui_file = std::env::temp_dir().join(format!("{name}-{}.json", std::process::id()));
        std::fs::write(&ui_file, contents).expect("write ui state");
        let (aws, _) = recording_aws(aws_regions());
        let (shell, cx) = shell_with_ui_state(cx, aws, Some(ui_file));
        cx.run_until_parked();
        (shell, cx)
    }

    #[gpui::test]
    fn switching_provider_reloads_the_context_dimensions(cx: &mut TestAppContext) {
        let (aws, _) = recording_aws(vec!["us-east-1".into(), "eu-west-1".into()]);
        let gcp: std::sync::Arc<dyn Provider> =
            std::sync::Arc::new(clouddeck_core::ComputeServiceProvider::new(
                "gcp",
                vec![clouddeck_core::Connection {
                    id: "default".into(),
                    display_name: "default".to_string(),
                }],
                vec!["europe-west1".into()],
                |_, _| Box::new(clouddeck_core::MockComputeService::new(Vec::new(), None)),
            ));
        install_providers(cx, vec![aws, gcp]);
        let (shell, cx) = add_rooted_shell(cx, |window, cx| {
            AppShell::build_with_profiles(
                window,
                true,
                DemoFleet::Happy,
                Ok(Vec::new()),
                None,
                Settings::default(),
                cx,
            )
        });
        assert_eq!(
            region_dimension_values(&shell, cx),
            ["us-east-1", "eu-west-1"]
        );

        let settings_file =
            std::env::temp_dir().join(format!("clouddeck-dimensions-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&settings_file);
        shell.update_in(cx, |shell, window, cx| {
            shell.switch_provider_to("gcp", &settings_file, window, cx)
        });
        cx.run_until_parked();

        assert_eq!(region_dimension_values(&shell, cx), ["europe-west1"]);
        let _ = std::fs::remove_file(&settings_file);
    }

    #[gpui::test]
    fn partial_region_selection_reaches_the_listing_call(cx: &mut TestAppContext) {
        let ui_file =
            std::env::temp_dir().join(format!("clouddeck-selection-{}.json", std::process::id()));
        UiState {
            dimension_selections: BTreeMap::from([(
                REGION.to_string(),
                vec!["us-west-2".into(), "eu-west-1".into()],
            )]),
            ..UiState::default()
        }
        .save(&ui_file)
        .expect("save ui state");
        let (aws, queried) = recording_aws(aws_regions());
        let (shell, cx) = shell_with_ui_state(cx, aws, Some(ui_file));
        cx.run_until_parked();

        assert_eq!(sorted_queries(&queried), ["eu-west-1", "us-west-2"]);
        shell.read_with(cx, |shell, _| {
            assert_eq!(
                shell.context_model.selection().values(REGION),
                [ValueId::from("us-west-2"), ValueId::from("eu-west-1")]
            );
        });
    }

    #[gpui::test]
    fn fleet_selection_queries_its_regions_and_counts_loaded_instances(cx: &mut TestAppContext) {
        let ui_file = std::env::temp_dir().join(format!(
            "clouddeck-fleet-selection-{}.json",
            std::process::id()
        ));
        UiState {
            dimension_selections: BTreeMap::from([(
                REGION.to_string(),
                FLEET_REGIONS.iter().map(|r| r.to_string()).collect(),
            )]),
            ..UiState::default()
        }
        .save(&ui_file)
        .expect("save ui state");
        let (aws, queried) = recording_aws(aws_regions());
        let (shell, cx) = shell_with_ui_state(cx, aws, Some(ui_file));
        cx.run_until_parked();

        let mut expected: Vec<String> = FLEET_REGIONS.iter().map(|r| r.to_string()).collect();
        expected.sort();
        assert_eq!(sorted_queries(&queried), expected);
        shell.read_with(cx, |shell, _| {
            assert_eq!(
                shell
                    .context_model
                    .value_count(REGION, &ValueId::from("us-west-2")),
                1
            );
        });
    }

    #[test]
    fn fleet_regions_follow_the_region_selection() {
        let available = AppShell::available_regions(false, DemoFleet::Happy);
        let mut model = ContextModel::new(Vec::new(), None, "aws");
        ensure_region_dimension(&mut model, &available);
        assert_eq!(AppShell::fleet_regions_for(&model), FLEET_REGIONS);
        set_region_values(&mut model, &["eu-west-1".to_string()]);
        assert_eq!(AppShell::fleet_regions_for(&model), ["eu-west-1"]);
    }

    #[test]
    fn demo_region_universe_extends_the_demo_list_with_every_aws_region() {
        for scenario in [
            DemoFleet::Happy,
            DemoFleet::Empty,
            DemoFleet::Failure,
            DemoFleet::Partial,
            DemoFleet::Large,
            DemoFleet::Huge,
        ] {
            let demo_list: Vec<String> = AppShell::regions(true, scenario)
                .into_iter()
                .filter(|region| region != "fleet")
                .collect();
            let universe = AppShell::available_regions(true, scenario);
            assert_eq!(&universe[..demo_list.len()], demo_list.as_slice());
            for region in clouddeck_core::REGIONS
                .iter()
                .filter(|region| **region != "fleet")
            {
                assert!(universe.iter().any(|known| known == region));
            }
            assert!(!universe.iter().any(|region| region == "fleet"));
            let unique: HashSet<&String> = universe.iter().collect();
            assert_eq!(unique.len(), universe.len());
        }
    }

    #[gpui::test]
    fn all_regions_are_not_saved_in_dimension_selections(cx: &mut TestAppContext) {
        let (aws, _) = recording_aws(aws_regions());
        let (shell, cx) = shell_with_ui_state(cx, aws, None);
        shell.update(cx, |shell, cx| {
            shell.context_model.select_all(REGION);
            shell.remembered_regions = Some(AppShell::remembered_regions_for(&shell.context_model));
            assert!(
                !shell
                    .ui_state_snapshot(cx)
                    .dimension_selections
                    .contains_key(REGION)
            );
        });
    }

    #[gpui::test]
    fn legacy_selected_regions_migrate_once_into_the_region_selection(cx: &mut TestAppContext) {
        let ui_file = std::env::temp_dir().join(format!(
            "clouddeck-legacy-selection-{}.json",
            std::process::id()
        ));
        std::fs::write(
            &ui_file,
            r#"{"selected_regions":["us-west-2","eu-west-1"]}"#,
        )
        .expect("write legacy ui state");
        let (aws, queried) = recording_aws(aws_regions());
        let (shell, cx) = shell_with_ui_state(cx, aws, Some(ui_file));
        cx.run_until_parked();

        assert_eq!(sorted_queries(&queried), ["eu-west-1", "us-west-2"]);
        shell.read_with(cx, |shell, _| {
            assert_eq!(
                region_values(&shell.context_model),
                ["us-west-2", "eu-west-1"]
            );
            assert!(shell.ui_state_saved.selected_regions.is_none());
        });
        shell.read_with(cx, |shell, cx| {
            let path = shell.ui_state_path.clone().expect("ui state path");
            shell
                .ui_state_snapshot(cx)
                .save(&path)
                .expect("save ui state");
            let saved = std::fs::read_to_string(&path).expect("read saved ui state");
            assert!(!saved.contains("selected_regions"));
            let _ = std::fs::remove_file(&path);
        });
    }

    #[gpui::test]
    fn stored_region_selection_wins_over_the_legacy_key(cx: &mut TestAppContext) {
        let ui_file = std::env::temp_dir().join(format!(
            "clouddeck-stored-selection-{}.json",
            std::process::id()
        ));
        std::fs::write(
            &ui_file,
            r#"{"dimension_selections":{"region":["us-west-2"]},"selected_regions":["eu-west-1"]}"#,
        )
        .expect("write ui state");
        let (aws, _) = recording_aws(aws_regions());
        let (shell, cx) = shell_with_ui_state(cx, aws, Some(ui_file));
        cx.run_until_parked();

        shell.read_with(cx, |shell, _| {
            assert_eq!(region_values(&shell.context_model), ["us-west-2"]);
        });
    }

    #[gpui::test]
    fn remembered_all_regions_restore_as_all_regions(cx: &mut TestAppContext) {
        let (shell, cx) =
            shell_restored_from_ui_file(cx, "clouddeck-remembered-all", r#"{"sidebar_open":true}"#);
        shell.read_with(cx, |shell, _| {
            assert!(shell.context_model.is_all(REGION));
        });
    }

    #[gpui::test]
    fn unreadable_ui_state_restores_the_default_regions_not_all(cx: &mut TestAppContext) {
        let (shell, cx) = shell_restored_from_ui_file(cx, "clouddeck-unreadable-ui", "{ not json");
        shell.read_with(cx, |shell, _| {
            assert!(!shell.context_model.is_all(REGION));
        });
    }

    fn scoped_provider_id(shell: &Entity<AppShell>, cx: &mut VisualTestContext) -> String {
        shell.read_with(cx, |shell, cx| {
            shell
                .fleet
                .read(cx)
                .view_model
                .provider_names()
                .into_keys()
                .next()
                .expect("one provider is listed")
        })
    }

    #[gpui::test]
    fn switching_provider_persists_the_choice_and_rebuilds_the_fleet(cx: &mut TestAppContext) {
        install_registry(cx, compute_provider("gcp"));
        let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);
        let settings_file =
            std::env::temp_dir().join(format!("clouddeck-switch-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&settings_file);
        let fleet_before = shell.read_with(cx, |shell, _| shell.fleet.entity_id());

        shell.update_in(cx, |shell, window, cx| {
            shell.switch_provider_to("gcp", &settings_file, window, cx)
        });
        cx.run_until_parked();

        shell.read_with(cx, |shell, _| {
            assert_eq!(shell.context_model.active_provider(), "gcp");
            assert_eq!(shell.context_model.connections().len(), 1);
        });
        assert_eq!(
            Settings::load(&settings_file).active_provider.as_deref(),
            Some("gcp")
        );
        let fleet_after = shell.read_with(cx, |shell, _| shell.fleet.entity_id());
        assert_ne!(fleet_before, fleet_after);

        shell.update_in(cx, |shell, window, cx| {
            shell.switch_provider_to("aws", &settings_file, window, cx)
        });
        cx.run_until_parked();

        shell.read_with(cx, |shell, _| {
            assert_eq!(shell.context_model.active_provider(), "aws");
        });
        assert_eq!(
            Settings::load(&settings_file).active_provider.as_deref(),
            Some("aws")
        );
        let _ = std::fs::remove_file(&settings_file);
    }

    #[gpui::test]
    fn sign_in_event_for_a_non_aws_provider_starts_its_flow(cx: &mut TestAppContext) {
        install_registry(cx, std::sync::Arc::new(SignedOut(compute_provider("gcp"))));
        let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);
        let main_window = shell.read_with(cx, |shell, _| shell.main_window.clone());

        main_window.update(cx, |_, cx| {
            cx.emit(MainWindowEvent::SignIn("gcp".to_string()))
        });

        shell.read_with(cx, |shell, _| {
            assert!(shell.sign_ins_in_flight.borrow().contains("gcp"));
            assert!(shell.wizard.is_none());
        });
    }

    #[gpui::test]
    fn switching_to_a_provider_that_needs_sign_in_shows_the_prompt(cx: &mut TestAppContext) {
        install_registry(cx, std::sync::Arc::new(SignedOut(compute_provider("gcp"))));
        let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);
        let settings_file =
            std::env::temp_dir().join(format!("clouddeck-sign-in-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&settings_file);

        shell.update_in(cx, |shell, window, cx| {
            shell.switch_provider_to("gcp", &settings_file, window, cx)
        });
        cx.run_until_parked();

        shell.read_with(cx, |shell, _| {
            assert!(shell.context_model.needs_sign_in());
        });
        let main_window = shell.read_with(cx, |shell, _| shell.main_window.clone());
        assert!(main_window.read_with(cx, |main_window, _| main_window.sign_in));
        let _ = std::fs::remove_file(&settings_file);
    }

    #[gpui::test]
    fn persisted_active_provider_is_used_at_launch(cx: &mut TestAppContext) {
        install_registry(cx, compute_provider("gcp"));
        let settings = Settings {
            active_provider: Some("gcp".to_string()),
            ..Settings::default()
        };
        let (shell, cx) = add_rooted_shell(cx, move |window, cx| {
            AppShell::build_with_profiles(
                window,
                false,
                DemoFleet::Happy,
                Ok(Vec::new()),
                None,
                settings,
                cx,
            )
        });

        shell.read_with(cx, |shell, _| {
            assert_eq!(shell.context_model.active_provider(), "gcp");
        });
        assert_eq!(scoped_provider_id(&shell, cx), "gcp");
    }

    struct Unauthenticated(std::sync::Arc<dyn Provider>);

    impl Provider for Unauthenticated {
        fn info(&self) -> clouddeck_core::ProviderInfo {
            self.0.info()
        }

        fn connections(
            &self,
            _cx: &clouddeck_core::Call<'_>,
        ) -> Result<Vec<clouddeck_core::Connection>, clouddeck_core::ProviderError> {
            Err(clouddeck_core::ProviderError::of(
                clouddeck_core::ErrorKind::Auth,
            ))
        }

        fn dimensions(
            &self,
            cx: &clouddeck_core::Call<'_>,
            connection: &clouddeck_core::ConnectionId,
        ) -> Result<Vec<clouddeck_core::Dimension>, clouddeck_core::ProviderError> {
            self.0.dimensions(cx, connection)
        }

        fn kinds(&self) -> Vec<clouddeck_core::KindInfo> {
            self.0.kinds()
        }

        fn list(
            &self,
            _cx: &clouddeck_core::Call<'_>,
            _connection: &clouddeck_core::ConnectionId,
            _kind: &clouddeck_core::KindId,
            _selection: &clouddeck_core::Selection,
        ) -> clouddeck_core::ListResult {
            clouddeck_core::ListResult {
                resources: Vec::new(),
                failures: vec![clouddeck_core::CellFailure {
                    cell: clouddeck_core::Selection::new(),
                    error: clouddeck_core::ProviderError::of(clouddeck_core::ErrorKind::Auth),
                }],
            }
        }

        fn console_url(
            &self,
            connection: &clouddeck_core::ConnectionId,
            kind: &clouddeck_core::KindId,
            resource: &clouddeck_core::Instance,
        ) -> Option<String> {
            self.0.console_url(connection, kind, resource)
        }

        fn fields(&self, kind: &clouddeck_core::KindId) -> Vec<clouddeck_core::FieldDef> {
            self.0.fields(kind)
        }
    }

    fn compute_with_instances(
        id: &str,
        instances: Vec<clouddeck_core::EC2Instance>,
    ) -> std::sync::Arc<dyn Provider> {
        std::sync::Arc::new(clouddeck_core::ComputeServiceProvider::new(
            id,
            vec![clouddeck_core::Connection {
                id: "default".into(),
                display_name: "default".to_string(),
            }],
            vec!["us-east-1".to_string()],
            move |_, _| {
                Box::new(clouddeck_core::MockComputeService::new(
                    instances.clone(),
                    None,
                ))
            },
        ))
    }

    fn failing_compute_provider(id: &str) -> std::sync::Arc<dyn Provider> {
        std::sync::Arc::new(clouddeck_core::ComputeServiceProvider::new(
            id,
            vec![clouddeck_core::Connection {
                id: "default".into(),
                display_name: "default".to_string(),
            }],
            vec!["us-east-1".to_string()],
            |_, _| {
                Box::new(clouddeck_core::MockComputeService::new(
                    Vec::new(),
                    Some("upstream unavailable".into()),
                ))
            },
        ))
    }

    fn build_shell(
        cx: &mut TestAppContext,
        settings: Settings,
    ) -> (Entity<AppShell>, &mut VisualTestContext) {
        add_rooted_shell(cx, move |window, cx| {
            AppShell::build_with_profiles(
                window,
                false,
                DemoFleet::Happy,
                Ok(Vec::new()),
                None,
                settings,
                cx,
            )
        })
    }

    fn wait_until_loaded(cx: &mut VisualTestContext, shell: &Entity<AppShell>) {
        for _ in 0..200 {
            cx.run_until_parked();
            let loading =
                shell.read_with(cx, |shell, cx| shell.fleet.read(cx).view_model.is_loading());
            if !loading {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("providers did not finish loading within the timeout");
    }

    fn all_providers_fleet(
        shell: &Entity<AppShell>,
        cx: &mut VisualTestContext,
    ) -> Entity<crate::fleet_state::FleetState> {
        shell.read_with(cx, |shell, _| shell.fleet.clone())
    }

    #[gpui::test]
    fn persisted_all_providers_loads_every_signed_in_provider(cx: &mut TestAppContext) {
        install_providers(
            cx,
            vec![
                compute_with_instances("aws", vec![bare_ec2("us-east-1", "i-1")]),
                compute_with_instances("gcp", vec![bare_ec2("us-east-1", "i-2")]),
            ],
        );
        let settings = Settings {
            active_provider: Some(ALL_PROVIDERS.to_string()),
            ..Settings::default()
        };
        let (shell, cx) = build_shell(cx, settings);
        wait_until_loaded(cx, &shell);

        shell.read_with(cx, |shell, _| {
            assert!(shell.context_model.shows_all_providers());
        });
        let fleet = all_providers_fleet(&shell, cx);
        fleet.read_with(cx, |fleet, _| {
            assert!(fleet.view_model.is_all());
            let names: Vec<String> = fleet.view_model.provider_names().into_keys().collect();
            assert_eq!(names, ["aws", "gcp"]);
            assert!(fleet.view_model.provider_failures().is_empty());
            let rows: Vec<String> = fleet
                .view_model
                .rows()
                .into_iter()
                .map(|(provider, _)| provider)
                .collect();
            assert_eq!(rows, ["aws", "gcp"]);
        });
    }

    #[gpui::test]
    fn failing_provider_is_a_failed_group_and_the_others_still_show(cx: &mut TestAppContext) {
        install_providers(
            cx,
            vec![
                compute_with_instances("aws", vec![bare_ec2("us-east-1", "i-1")]),
                failing_compute_provider("gcp"),
            ],
        );
        let settings = Settings {
            active_provider: Some(ALL_PROVIDERS.to_string()),
            ..Settings::default()
        };
        let (shell, cx) = build_shell(cx, settings);
        wait_until_loaded(cx, &shell);

        let fleet = all_providers_fleet(&shell, cx);
        fleet.read_with(cx, |fleet, _| {
            let failures = fleet.view_model.provider_failures();
            assert_eq!(failures.len(), 1);
            assert_eq!(failures[0].provider_id, "gcp");
            assert!(!failures[0].needs_sign_in);
            let rows: Vec<String> = fleet
                .view_model
                .rows()
                .into_iter()
                .map(|(provider, _)| provider)
                .collect();
            assert_eq!(rows, ["aws"]);
        });
    }

    #[gpui::test]
    fn signed_out_provider_is_listed_as_needing_sign_in(cx: &mut TestAppContext) {
        install_providers(
            cx,
            vec![
                compute_provider("aws"),
                std::sync::Arc::new(Unauthenticated(compute_provider("gcp"))),
            ],
        );
        let settings = Settings {
            active_provider: Some(ALL_PROVIDERS.to_string()),
            ..Settings::default()
        };
        let (shell, cx) = build_shell(cx, settings);
        wait_until_loaded(cx, &shell);

        let fleet = all_providers_fleet(&shell, cx);
        fleet.read_with(cx, |fleet, _| {
            let failures = fleet.view_model.provider_failures();
            let gcp = failures
                .iter()
                .find(|failure| failure.provider_id == "gcp")
                .expect("gcp is listed");
            assert!(gcp.needs_sign_in);
            assert!(failures.iter().all(|failure| failure.provider_id != "aws"));
        });
    }

    #[gpui::test]
    fn switching_all_then_aws_then_all_keeps_the_choice(cx: &mut TestAppContext) {
        install_registry(cx, compute_provider("gcp"));
        let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);
        let settings_file =
            std::env::temp_dir().join(format!("clouddeck-all-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&settings_file);

        shell.update_in(cx, |shell, window, cx| {
            shell.switch_provider_to(ALL_PROVIDERS, &settings_file, window, cx)
        });
        cx.run_until_parked();
        let fleet = all_providers_fleet(&shell, cx);
        assert!(fleet.read_with(cx, |fleet, _| fleet.view_model.is_all()));

        shell.update_in(cx, |shell, window, cx| {
            shell.switch_provider_to("aws", &settings_file, window, cx)
        });
        cx.run_until_parked();
        shell.read_with(cx, |shell, _| {
            assert_eq!(shell.context_model.active_provider(), "aws");
        });
        let fleet = all_providers_fleet(&shell, cx);
        assert!(!fleet.read_with(cx, |fleet, _| fleet.view_model.is_all()));

        shell.update_in(cx, |shell, window, cx| {
            shell.switch_provider_to(ALL_PROVIDERS, &settings_file, window, cx)
        });
        cx.run_until_parked();
        assert_eq!(
            Settings::load(&settings_file).active_provider.as_deref(),
            Some(ALL_PROVIDERS)
        );
        let fleet = all_providers_fleet(&shell, cx);
        assert!(fleet.read_with(cx, |fleet, _| fleet.view_model.is_all()));
        let _ = std::fs::remove_file(&settings_file);
    }

    #[gpui::test]
    fn unregistered_provider_id_leaves_the_active_provider_unchanged(cx: &mut TestAppContext) {
        let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);
        let before = shell.read_with(cx, |shell, _| {
            shell.context_model.active_provider().to_string()
        });
        let main_window = shell.read_with(cx, |shell, _| shell.main_window.clone());

        main_window.update(cx, |_, cx| {
            cx.emit(MainWindowEvent::PickProvider(
                "no-such-provider".to_string(),
            ))
        });
        cx.run_until_parked();

        let after = shell.read_with(cx, |shell, _| {
            shell.context_model.active_provider().to_string()
        });
        assert_eq!(before, after);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clouddeck_core::fixture_fleet::{Gate, gated_provider};
    use gpui::VisualTestContext;
    use gpui::{AppContext, TestAppContext};

    fn parse(args: &[&str]) -> Result<LaunchOptions, String> {
        parse_launch_options(args.iter().map(|arg| arg.to_string()))
    }

    #[test]
    fn size_flag_parses_width_and_height() {
        let options = parse(&["--size", "520x760"]).unwrap();
        assert_eq!(options.size, Some((520.0, 760.0)));
        assert!(!options.demo);
    }

    #[test]
    fn invalid_sizes_are_rejected() {
        for bad in ["520", "520x", "x760", "0x760", "axb", "520X760", "-1x5"] {
            assert!(parse(&["--size", bad]).is_err(), "{bad}");
        }
        assert!(parse(&["--size"]).is_err());
    }

    #[test]
    fn unknown_render_state_lists_the_valid_names() {
        let error = parse(&["--render-state", "Nope"]).unwrap_err();
        assert!(error.contains("Nope") && error.contains("OverlayPalette"));
    }

    #[test]
    fn render_state_implies_demo_and_its_scenario() {
        let options = parse(&["--render-state", "MainEmpty", "--size", "800x600"]).unwrap();
        assert_eq!(options.state, Some(State::MainEmpty));
        assert!(options.demo);
        assert_eq!(options.scenario, DemoFleet::Empty);
        assert_eq!(options.size, Some((800.0, 600.0)));
    }

    #[test]
    fn huge_demo_scenario_is_parsed() {
        let options = parse(&["--demo-scenario", "huge"]).unwrap();
        assert_eq!(options.scenario, DemoFleet::Huge);
        assert!(options.demo);
    }

    #[test]
    fn explicit_demo_scenario_beats_the_state_scenario() {
        let options = parse(&["--demo-scenario", "large", "--render-state", "MainEmpty"]).unwrap();
        assert_eq!(options.scenario, DemoFleet::Large);
    }

    #[test]
    fn version_flag_sets_print_version() {
        assert!(parse(&["--version"]).unwrap().print_version);
    }

    #[test]
    fn short_version_flag_sets_print_version() {
        assert!(parse(&["-V"]).unwrap().print_version);
    }

    #[test]
    fn version_flag_combines_with_demo() {
        let options = parse(&["--demo", "--version"]).unwrap();
        assert!(options.print_version);
        assert!(options.demo);
    }

    #[test]
    fn no_flags_leave_print_version_off() {
        assert!(!parse(&[]).unwrap().print_version);
    }

    #[test]
    fn usage_line_lists_version_flag() {
        assert!(USAGE.contains("--version"));
    }

    #[test]
    fn every_state_name_round_trips() {
        for state in State::ALL {
            assert_eq!(State::from_name(state.name()), Some(state));
        }
    }

    // clouddeck-rs-mac-ui-fix-ellipsis: the Name and ID cells of the fleet
    // table are reported to render only an ellipsis ("...") in the running
    // app at 1280pt. The tests build the real AppShell (the live --demo
    // mount path, in both the 3-row demo and the 121-row large scenario),
    // resize to the reported 1280x800, and assert the name cell keeps at
    // least half the row width (and the ID cell non-zero) once the async
    // demo load has landed. The headless layout measures healthy, so these
    // are regression guards for the AppShell-embedded layout; a manual
    // macOS --demo check at 1280pt is still the ground truth for the
    // reported symptom.
    fn assert_appshell_row_widths(cx: &mut VisualTestContext, row_key: &'static str) {
        let row = cx
            .debug_bounds(row_key)
            .unwrap_or_else(|| panic!("row {row_key} should render"));
        let name_key = format!("{row_key}.name");
        // `debug_bounds` only accepts `&'static str`; leak a static copy
        // (tests run in short-lived processes).
        let name_static: &'static str = Box::leak(name_key.into_boxed_str());
        let name = cx
            .debug_bounds(name_static)
            .unwrap_or_else(|| panic!("name cell {name_static} should render"));
        let id_key = format!("{row_key}.id");
        let id_static: &'static str = Box::leak(id_key.into_boxed_str());
        let id = cx
            .debug_bounds(id_static)
            .unwrap_or_else(|| panic!("id cell {id_static} should render"));

        // Name is flex_1 with a 170pt minimum; the fixed columns take the rest,
        // so a half-row share is not expected once all eight columns are shown.
        assert!(
            name.size.width >= px(170.0),
            "name cell should keep its minimum width (got {name} of {row})"
        );
        assert!(
            id.size.width > px(0.0),
            "id cell must not be zero width (got {id})"
        );
        // The regression symptom: the first column pinned to the entire row
        // width, starving the fixed Region/Type/State columns (which total
        // 240pt plus gaps/padding at this window width).
        assert!(
            name.size.width < row.size.width - px(100.0),
            "name cell must not fill the whole row (the fixed columns are \
             starved) (got {name} of {row})"
        );
    }

    /// Mounts the shell the way `main.rs` does: gpui-component is initialised
    /// and the shell is wrapped in a `Root`, so its widgets have a theme.
    fn add_rooted_shell(
        cx: &mut TestAppContext,
        build: impl FnOnce(&mut Window, &mut Context<AppShell>) -> AppShell + 'static,
    ) -> (Entity<AppShell>, &mut VisualTestContext) {
        let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
        let slot_for_window = slot.clone();
        let (_root, cx) = cx.add_window_view(move |window, cx| {
            gpui_component::init(cx);
            let shell = cx.new(|cx| build(window, cx));
            *slot_for_window.borrow_mut() = Some(shell.clone());
            gpui_component::Root::new(shell, window, cx)
        });
        let shell = slot
            .borrow()
            .as_ref()
            .expect("rooted shell entity should be created")
            .clone();
        (shell, cx)
    }

    /// Polls `run_until_parked` + short sleeps until `selector` renders,
    /// mirroring the startup-load pattern the other AppShell tests use
    /// (the demo load crosses onto the shared tokio runtime, so one
    /// `run_until_parked()` can return before the rows land).
    fn wait_for_row(cx: &mut VisualTestContext, selector: &str) {
        for _ in 0..200 {
            cx.run_until_parked();
            let static_selector: &'static str = Box::leak(selector.to_string().into_boxed_str());
            if cx.debug_bounds(static_selector).is_some() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("row {selector} did not render within the timeout");
    }

    #[gpui::test]
    fn instance_row_name_and_id_cells_keep_readable_width_in_appshell_demo(
        cx: &mut TestAppContext,
    ) {
        let (_view, cx) = add_rooted_shell(cx, AppShell::new_demo);
        // The reported regression is at 1280pt window width.
        cx.simulate_resize(gpui::size(px(1280.0), px(800.0)));
        wait_for_row(cx, "table.row.us-east-1:i-scenario-001");

        assert_appshell_row_widths(cx, "table.row.us-east-1:i-scenario-001");
        assert_appshell_row_widths(cx, "table.row.us-east-1:i-scenario-002");
    }

    #[gpui::test]
    fn instance_row_name_and_id_cells_keep_readable_width_in_appshell_large(
        cx: &mut TestAppContext,
    ) {
        let (_view, cx) = add_rooted_shell(cx, AppShell::new_demo_large);
        cx.simulate_resize(gpui::size(px(1280.0), px(800.0)));
        // 121 rows are in the model, but uniform_list only draws the visible
        // ones. Sorted by Name ascending, i-scenario-003 (bastion-east-1),
        // i-scenario-002 (batch-worker-1) and i-demo-010 (demo-10) are the
        // first three rows; i-scenario-001 (prod-api-1) sorts last.
        wait_for_row(cx, "table.row.us-east-1:i-scenario-002");

        assert_appshell_row_widths(cx, "table.row.us-east-1:i-scenario-003");
        assert_appshell_row_widths(cx, "table.row.us-east-1:i-demo-010");
    }

    #[gpui::test]
    fn a_superseded_context_load_does_not_overwrite_the_current_context(cx: &mut TestAppContext) {
        let gate = Gate::new();
        let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);
        wait_for_row(cx, "table.row.us-east-1:i-scenario-001");

        shell.update(cx, |shell, _| {
            shell.set_test_service_override(gated_provider(vec![], None, gate.clone()));
        });
        emit_main_window_event(
            cx,
            &shell,
            MainWindowEvent::RegionsChanged(vec!["us-west-2".into()]),
        );

        shell.update(cx, |shell, _| shell.test_service_override = None);
        emit_main_window_event(
            cx,
            &shell,
            MainWindowEvent::RegionsChanged(vec!["us-east-1".into()]),
        );
        wait_until(cx, "the current context load to settle", |cx| {
            shell.read_with(cx, |shell, cx| {
                !shell.fleet.read(cx).view_model.is_loading()
            })
        });

        gate.open();
        for _ in 0..20 {
            cx.run_until_parked();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        assert!(
            cx.debug_bounds("table.row.us-east-1:i-scenario-001")
                .is_some(),
            "the superseded load must not clear the current context's rows"
        );
    }

    #[gpui::test]
    fn mounts_without_a_display(cx: &mut TestAppContext) {
        let (_view, cx) = add_rooted_shell(cx, AppShell::new_demo);

        assert!(
            cx.debug_bounds("app-shell").is_some(),
            "app-shell root should render"
        );
    }

    #[gpui::test]
    fn renders_main_window_regions(cx: &mut TestAppContext) {
        let (_view, cx) = add_rooted_shell(cx, AppShell::new_demo);
        wait_for_row(cx, "table.row.us-east-1:i-scenario-001");

        assert!(
            cx.debug_bounds("main.center").is_some(),
            "the main window center should render"
        );
        assert!(
            cx.debug_bounds("main.table").is_some(),
            "the instance table should be mounted in the main window center"
        );
        assert!(
            cx.debug_bounds("main.statusbar").is_some(),
            "the main window status bar should render"
        );
    }

    #[gpui::test]
    fn main_table_shows_the_seeded_sample_instances(cx: &mut TestAppContext) {
        let (_view, cx) = add_rooted_shell(cx, AppShell::new_demo);
        wait_for_row(cx, "table.row.us-east-1:i-scenario-001");

        assert!(
            cx.debug_bounds("main.table").is_some(),
            "the seeded sample instances should render in the main table"
        );
    }

    #[gpui::test]
    fn startup_load_is_async_and_transitions_from_loading_to_populated(cx: &mut TestAppContext) {
        // Mirrors the startup path: the fleet entity is built and refreshed,
        // so the load is spawned instead of blocking the UI thread on
        // `cx.background_executor().block(...)`.
        let gate = Gate::new();
        let provider = gated_provider(
            clouddeck_core::fixture_fleet::instances(&[("i-gated", "", "", "")]),
            None,
            gate.clone(),
        );
        let view_model = InstanceListViewModel::new(provider);
        let fleet = cx.new(|_| FleetState::new(view_model, "scenario-default".into(), false));
        fleet.update(cx, |fleet, cx| fleet.refresh(cx));

        assert!(
            fleet.read_with(cx, |fleet, _| fleet.view_model.is_loading()),
            "the view model should report loading while the spawn is in flight"
        );
        assert!(
            fleet.read_with(cx, |fleet, _| fleet.view_model.instances().is_empty()),
            "data should not be populated synchronously while the gated load is in flight"
        );

        gate.open();
        let mut loaded = false;
        for _ in 0..200 {
            cx.run_until_parked();
            if fleet.read_with(cx, |fleet, _| {
                !fleet.view_model.is_loading() && !fleet.view_model.instances().is_empty()
            }) {
                loaded = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(loaded, "the load did not complete within the timeout");
        assert!(
            fleet.read_with(cx, |fleet, _| fleet
                .view_model
                .instances()
                .iter()
                .any(|instance| clouddeck_core::native_id(instance) == "i-gated")),
            "the gated instance should be populated once the async load completes"
        );
    }

    #[gpui::test]
    fn main_center_and_status_bar_are_docked_in_the_window(cx: &mut TestAppContext) {
        let (_view, cx) = add_rooted_shell(cx, AppShell::new_demo);
        wait_for_row(cx, "table.row.us-east-1:i-scenario-001");

        let center = cx
            .debug_bounds("main.center")
            .expect("main center should render");
        let status_bar = cx
            .debug_bounds("main.statusbar")
            .expect("main status bar should render");

        assert!(
            status_bar.origin.y >= center.origin.y + center.size.height
                || status_bar.origin.x >= center.origin.x + center.size.width,
            "the status bar should be docked beside or below the main center"
        );
        assert!(
            status_bar.size.height > px(0.0),
            "status bar should have a non-zero height, not collapse to nothing"
        );
    }

    #[test]
    fn app_shortcuts_use_standard_secondary_keys() {
        let bindings = app_key_bindings();
        assert_eq!(bindings.len(), 2);
        assert!(
            bindings
                .iter()
                .any(|binding| binding.action().name().ends_with("::Quit"))
        );
        assert!(
            bindings
                .iter()
                .any(|binding| binding.action().name().ends_with("::Refresh"))
        );
    }

    #[test]
    fn demo_regions_only_include_fixture_regions_and_fleet() {
        assert_eq!(
            AppShell::regions(true, DemoFleet::Happy),
            vec!["fleet".to_string(), "us-east-1".to_string()]
        );
        assert!(
            AppShell::regions(true, DemoFleet::Happy)
                .iter()
                .all(|region| region == "fleet" || region == "us-east-1")
        );
    }

    #[test]
    fn demo_failure_and_partial_modes_are_observable_without_aws() {
        let context = WorkspaceContext {
            profile: "scenario-default".into(),
        };

        let fleet_regions: Vec<String> = FLEET_REGIONS.iter().map(|r| (*r).into()).collect();
        let failure = loaded_demo(&context, DemoFleet::Failure, &fleet_regions);
        assert!(
            failure
                .error_message()
                .is_some_and(|message| message.contains("Simulated demo load failure")),
            "failure scenario should fail visibly"
        );

        let partial = loaded_demo(&context, DemoFleet::Partial, &fleet_regions);
        assert_eq!(partial.instances().len(), 3);
        assert_eq!(partial.partial_failures().len(), 3);
    }

    fn loaded_demo(
        context: &WorkspaceContext,
        demo: DemoFleet,
        fleet_regions: &[String],
    ) -> InstanceListViewModel {
        let registry = clouddeck_core::extension_host::app_registry(
            None,
            std::path::Path::new("/nonexistent-clouddeck-extensions"),
            std::path::Path::new("/nonexistent-clouddeck-state"),
            vec![test_aws_provider()],
        );
        let provider = AppShell::provider_for(
            registry.providers(),
            context,
            true,
            demo,
            fleet_regions,
            Duration::from_secs(5),
        );
        let mut view_model = InstanceListViewModel::new(provider);
        crate::runtime::runtime().block_on(view_model.load_instances());
        view_model
    }

    #[test]
    fn large_demo_mode_has_enough_rows_to_exercise_scrolling() {
        let context = WorkspaceContext {
            profile: "scenario-default".into(),
        };
        let view_model = loaded_demo(&context, DemoFleet::Large, &[]);
        assert_eq!(view_model.instances().len(), 120);
    }

    #[gpui::test]
    fn opening_setup_creates_the_wizard_and_finishing_it_updates_the_context(
        cx: &mut TestAppContext,
    ) {
        use crate::account_window::AccountConnectionState;

        let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);

        // The trigger: nothing opens the account-connect window on its own
        // (per clouddeck-rs-w6s.18's design, existing profiles need minimal
        // interaction), but `open_setup` -- reached from the main window's
        // `ConnectToAws` event -- must actually create a wizard window.
        shell.update(cx, |shell, cx| {
            assert!(shell.wizard.is_none(), "no wizard until setup is opened");
            shell.open_setup(cx);
            assert!(
                shell.wizard.is_some(),
                "open_setup should create the Connect to AWS wizard entity"
            );
        });

        let wizard = shell
            .read_with(cx, |shell, _| shell.wizard.clone())
            .expect("wizard entity");

        // Drive the wizard to a finished connection the same way a real
        // connect (account_connect::drive) would, then confirm the shell's
        // observer applies the selected profile and region as the context.
        wizard.update(cx, |wizard, cx| {
            wizard.selected_profile = Some("scenario-default".into());
            wizard.selected_region = "us-west-2".into();
            wizard.connection_state = AccountConnectionState::Connected {
                account_id: "123456789012".into(),
            };
            wizard.open_fleet();
            cx.notify();
        });

        shell.read_with(cx, |shell, _| {
            assert_eq!(
                shell.context.profile, "scenario-default",
                "finishing the wizard should apply its selected profile to the context"
            );
            assert_eq!(region_values(&shell.context_model), ["us-west-2"]);
        });
    }

    #[gpui::test]
    fn mounting_in_real_mode_without_profiles_shows_setup_affordance(cx: &mut TestAppContext) {
        // A real (non-demo) launch with no detected AWS profiles must reach
        // the main window with an in-place "Connect to AWS" affordance (the
        // no-profiles onboarding entry point, clouddeck-rs-w6s.18) rather
        // than a dead end. The load error renders as a centered state whose
        // action is that affordance.
        let real_mode = crate::shell::is_real_mode();
        let ui_state_path = temp_ui_state_path("mount-real-mode");
        let build_path = ui_state_path.clone();
        let (shell, cx) = add_rooted_shell(cx, move |window, cx| {
            if real_mode {
                AppShell::build_with_profiles(
                    window,
                    false,
                    DemoFleet::Happy,
                    Ok(Vec::new()),
                    Some(build_path),
                    Settings::default(),
                    cx,
                )
            } else {
                AppShell::build_with_scenario(window, true, DemoFleet::Empty, cx)
            }
        });

        wait_until(cx, "the load to settle", |cx| {
            shell.read_with(cx, |shell, cx| {
                !shell.fleet.read(cx).view_model.is_loading()
            })
        });
        let mut saw_setup = false;
        for _ in 0..200 {
            cx.run_until_parked();
            if cx.debug_bounds("center.action").is_some() {
                saw_setup = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        assert!(
            cx.debug_bounds("main.center-state").is_some(),
            "the centered state should render when the load has no instances"
        );
        if real_mode {
            assert!(
                saw_setup,
                "a real-mode launch with no profiles should show Connect to AWS"
            );
        } else {
            assert!(
                !saw_setup,
                "the demo empty scenario must not offer AWS onboarding"
            );
        }
        if let Some(dir) = ui_state_path.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    fn temp_ui_state_path(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("clouddeck-shell-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("ui-state.json")
    }

    #[gpui::test]
    fn saved_ui_state_is_restored_on_startup(cx: &mut TestAppContext) {
        let path = temp_ui_state_path("restore");
        UiState {
            dimension_selections: BTreeMap::from([(
                REGION.to_string(),
                vec!["us-east-1".into(), "eu-west-1".into()],
            )]),
            pinned_searches: vec!["state:stopped".into()],
            recent_searches: vec!["owner:esteban".into()],
            recent_instances: vec!["us-east-1:i-1".into()],
            sidebar_open: false,
            inspector_open: false,
            ..UiState::default()
        }
        .save(&path)
        .unwrap();

        let build_path = path.clone();
        let (shell, cx) = add_rooted_shell(cx, move |window, cx| {
            AppShell::build(window, true, DemoFleet::Partial, Some(build_path), cx)
        });

        shell.read_with(cx, |shell, cx| {
            let fleet = shell.fleet.read(cx);
            assert_eq!(fleet.pinned_searches, vec!["state:stopped".to_string()]);
            assert_eq!(fleet.recent_searches, vec!["owner:esteban".to_string()]);
            assert_eq!(fleet.recent_instances, vec!["us-east-1:i-1".to_string()]);
            assert!(!fleet.sidebar_open);
            assert!(!fleet.inspector_open);
            assert_eq!(
                region_values(&shell.context_model),
                ["us-east-1".to_string(), "eu-west-1".to_string()]
            );
            assert_eq!(
                region_values(&shell.main_window.read(cx).context_model),
                ["us-east-1".to_string(), "eu-west-1".to_string()]
            );
        });
        if let Some(dir) = path.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[gpui::test]
    fn ui_state_saves_once_the_layout_settles(cx: &mut TestAppContext) {
        let path = temp_ui_state_path("debounce");
        let build_path = path.clone();
        let (shell, cx) = add_rooted_shell(cx, move |window, cx| {
            AppShell::build(window, true, DemoFleet::Happy, Some(build_path), cx)
        });
        wait_until(cx, "the startup load to settle", |cx| {
            shell.read_with(cx, |shell, cx| {
                !shell.fleet.read(cx).view_model.is_loading()
            })
        });

        let sidebar_before = shell.read_with(cx, |shell, cx| shell.fleet.read(cx).sidebar_open);
        shell.update(cx, |shell, cx| {
            shell.fleet.update(cx, |fleet, cx| {
                fleet.pinned_searches.push("state:stopped".into());
                fleet.sidebar_open = !fleet.sidebar_open;
                cx.notify();
            });
        });
        cx.executor().advance_clock(Duration::from_millis(900));
        cx.run_until_parked();
        let saved_early = UiState::load(&path);
        assert!(
            !saved_early
                .pinned_searches
                .contains(&"state:stopped".to_string()),
            "the save should wait out the debounce"
        );
        assert!(
            !path.exists(),
            "nothing should be written before the debounce elapses"
        );

        cx.executor().advance_clock(Duration::from_millis(200));
        cx.run_until_parked();
        let saved = UiState::load(&path);
        assert_eq!(saved.pinned_searches, vec!["state:stopped".to_string()]);
        assert_eq!(saved.sidebar_open, !sidebar_before);
        if let Some(dir) = path.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    fn test_aws_provider() -> std::sync::Arc<dyn clouddeck_core::Provider> {
        std::sync::Arc::new(clouddeck_core::ComputeServiceProvider::new(
            "aws",
            Vec::new(),
            Vec::new(),
            |_, _| Box::new(clouddeck_core::MockComputeService::new(Vec::new(), None)),
        ))
    }

    fn install_test_providers(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let registry = clouddeck_core::extension_host::app_registry(
                None,
                std::path::Path::new("/nonexistent-clouddeck-extensions"),
                std::path::Path::new("/nonexistent-clouddeck-state"),
                vec![test_aws_provider()],
            );
            cx.set_global(crate::app_providers::AppProviders::new(registry));
            crate::app_providers::observe_quit(cx);
        });
    }

    fn provider_shutdowns(cx: &TestAppContext) -> usize {
        cx.read(|cx| {
            cx.global::<crate::app_providers::AppProviders>()
                .shutdowns()
        })
    }

    #[gpui::test]
    fn quit_shuts_the_providers_down_once(cx: &mut TestAppContext) {
        install_test_providers(cx);
        cx.update(|cx| cx.shutdown());
        assert_eq!(provider_shutdowns(cx), 1);
    }

    #[cfg(target_os = "linux")]
    #[gpui::test]
    fn closing_the_main_window_shuts_the_providers_down(cx: &mut TestAppContext) {
        install_test_providers(cx);
        let (_shell, cx) = add_rooted_shell(cx, |window, cx| {
            AppShell::build(window, true, DemoFleet::Happy, None, cx)
        });
        assert!(cx.simulate_close());
        assert_eq!(provider_shutdowns(&cx.cx), 1);
    }

    #[gpui::test]
    fn pending_ui_state_is_flushed_on_quit(cx: &mut TestAppContext) {
        let path = temp_ui_state_path("quit-flush");
        let build_path = path.clone();
        let (shell, cx) = add_rooted_shell(cx, move |window, cx| {
            AppShell::build(window, true, DemoFleet::Happy, Some(build_path), cx)
        });
        wait_until(cx, "the startup load to settle", |cx| {
            shell.read_with(cx, |shell, cx| {
                !shell.fleet.read(cx).view_model.is_loading()
            })
        });

        shell.update(cx, |shell, cx| {
            shell.fleet.update(cx, |fleet, cx| {
                fleet.sidebar_open = !fleet.sidebar_open;
                cx.notify();
            });
        });
        let sidebar_open = shell.read_with(cx, |shell, cx| shell.fleet.read(cx).sidebar_open);
        cx.run_until_parked();
        assert!(
            !path.exists(),
            "the change should still be inside the debounce window"
        );

        cx.cx.update(|cx| cx.shutdown());

        assert_eq!(UiState::load(&path).sidebar_open, sidebar_open);
        if let Some(dir) = path.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[gpui::test]
    fn hidden_columns_are_restored_and_saved(cx: &mut TestAppContext) {
        let path = temp_ui_state_path("hidden-columns");
        UiState {
            hidden_columns: vec!["az".into()],
            ..UiState::default()
        }
        .save(&path)
        .unwrap();
        let build_path = path.clone();
        let (shell, cx) = add_rooted_shell(cx, move |window, cx| {
            AppShell::build(window, true, DemoFleet::Happy, Some(build_path), cx)
        });
        assert_eq!(
            shell.read_with(cx, |shell, cx| shell.fleet.read(cx).hidden_columns.clone()),
            [Column::Az]
        );

        shell.update(cx, |shell, cx| {
            shell.fleet.update(cx, |fleet, cx| {
                fleet.toggle_column(Column::Launched, cx);
            });
            shell.save_ui_state(cx);
        });
        assert_eq!(UiState::load(&path).hidden_columns, ["az", "launched"]);
        if let Some(dir) = path.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[gpui::test]
    fn corrupt_ui_state_is_not_overwritten_by_autosave(cx: &mut TestAppContext) {
        let path = temp_ui_state_path("corrupt-autosave");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"not json").unwrap();
        let build_path = path.clone();
        let (shell, cx) = add_rooted_shell(cx, move |window, cx| {
            AppShell::build(window, true, DemoFleet::Happy, Some(build_path), cx)
        });
        wait_until(cx, "the startup load to settle", |cx| {
            shell.read_with(cx, |shell, cx| {
                !shell.fleet.read(cx).view_model.is_loading()
            })
        });

        shell.update(cx, |shell, cx| {
            shell.fleet.update(cx, |fleet, cx| {
                fleet.sidebar_open = !fleet.sidebar_open;
                cx.notify();
            });
        });
        shell.read_with(cx, |shell, _| {
            assert!(
                shell
                    .message
                    .as_deref()
                    .is_some_and(|message| message.contains("UI state")),
                "an unreadable UI state file should be reported in the shell"
            );
        });
        cx.executor().advance_clock(Duration::from_millis(1100));
        cx.run_until_parked();
        assert_eq!(std::fs::read(&path).unwrap(), b"not json");

        cx.cx.update(|cx| cx.shutdown());
        assert_eq!(std::fs::read(&path).unwrap(), b"not json");
        if let Some(dir) = path.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[gpui::test]
    fn settings_clears_are_not_written_back_by_the_next_save(cx: &mut TestAppContext) {
        let path = temp_ui_state_path("settings-clears");
        UiState {
            dimension_selections: BTreeMap::from([(
                REGION.to_string(),
                vec!["us-east-1".into(), "eu-west-1".into()],
            )]),
            recent_searches: vec!["owner:esteban".into()],
            ..UiState::default()
        }
        .save(&path)
        .unwrap();
        let build_path = path.clone();
        let (shell, cx) = add_rooted_shell(cx, move |window, cx| {
            AppShell::build(window, true, DemoFleet::Partial, Some(build_path), cx)
        });

        let mut on_disk = UiState::load(&path);
        on_disk.recent_searches.clear();
        on_disk.dimension_selections.clear();
        on_disk.save(&path).unwrap();
        shell.update(cx, |shell, cx| {
            shell.handle_settings_event(&SettingsEvent::RecentSearchesCleared, cx);
            shell.handle_settings_event(&SettingsEvent::RememberedScopeForgotten, cx);
            shell.fleet.update(cx, |fleet, cx| {
                fleet.sidebar_open = !fleet.sidebar_open;
                cx.notify();
            });
            shell.save_ui_state(cx);
        });

        let saved = UiState::load(&path);
        assert!(saved.recent_searches.is_empty());
        assert!(saved.dimension_selections.is_empty());
        shell.read_with(cx, |shell, cx| {
            assert!(shell.fleet.read(cx).recent_searches.is_empty());
            assert_eq!(
                region_values(&shell.context_model),
                ["us-east-1".to_string(), "eu-west-1".to_string()]
            );
        });

        shell.update(cx, |shell, cx| {
            shell.fleet.update(cx, |fleet, cx| {
                fleet.recent_searches = vec!["owner:esteban".into()];
                cx.notify();
            });
            shell.save_ui_state(cx);
        });
        assert_eq!(
            UiState::load(&path).recent_searches,
            vec!["owner:esteban".to_string()]
        );
        if let Some(dir) = path.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[gpui::test]
    fn region_scope_limits_the_fleet_query(cx: &mut TestAppContext) {
        let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo_partial);
        wait_until_context_settled(cx, &shell);

        let selected = vec!["us-east-1".to_string(), "eu-west-1".to_string()];
        emit_main_window_event(cx, &shell, MainWindowEvent::RegionsChanged(selected));
        wait_until_context_settled(cx, &shell);

        shell.read_with(cx, |shell, cx| {
            let vm = &shell.fleet.read(cx).view_model;
            assert_eq!(vm.partial_failures().len(), 1);
            assert_eq!(vm.partial_failures()[0].region, "eu-west-1");
            assert!(
                vm.instances()
                    .iter()
                    .all(|instance| instance.region.as_deref() == Some("us-east-1"))
            );
        });
    }

    #[gpui::test]
    fn connect_to_aws_event_opens_the_wizard(cx: &mut TestAppContext) {
        let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);

        emit_main_window_event(cx, &shell, MainWindowEvent::ConnectToAws);

        shell.read_with(cx, |shell, _| {
            assert!(
                shell.wizard.is_some(),
                "the ConnectToAws event should open the Connect to AWS window"
            );
        });
    }

    #[gpui::test]
    fn sign_in_event_for_aws_opens_the_wizard(cx: &mut TestAppContext) {
        let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);

        emit_main_window_event(cx, &shell, MainWindowEvent::SignIn("aws".to_string()));

        shell.read_with(cx, |shell, _| {
            assert!(
                shell.wizard.is_some(),
                "SignIn(aws) should open the AWS wizard"
            );
        });
    }

    #[gpui::test]
    fn sign_in_event_for_an_unknown_provider_flashes_and_opens_nothing(cx: &mut TestAppContext) {
        let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);

        emit_main_window_event(cx, &shell, MainWindowEvent::SignIn("nope".to_string()));

        shell.read_with(cx, |shell, cx| {
            assert!(shell.wizard.is_none());
            assert_eq!(
                shell.fleet.read(cx).flash.as_deref(),
                Some("nope is not installed")
            );
        });
    }

    // Regression test for a bug where the wizard's "Explore demo fleet"
    // action (`demo_requested`) was read and cleared by
    // `account_connect::drive` -- which never acts on it -- every render
    // cycle, so the button was a silent no-op. The wizard's observer must
    // intercept `demo_requested` itself and flip the shell's demo mode.
    #[gpui::test]
    fn wizard_demo_request_toggles_the_shell_demo_mode(cx: &mut TestAppContext) {
        let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);

        shell.update(cx, |shell, cx| shell.open_setup(cx));

        let wizard = shell
            .read_with(cx, |shell, _| shell.wizard.clone())
            .expect("wizard entity");
        let demo_before = shell.read_with(cx, |shell, _| shell.demo);

        wizard.update(cx, |wizard, cx| {
            wizard.request_demo();
            cx.notify();
        });

        shell.read_with(cx, |shell, _| {
            assert_ne!(
                shell.demo, demo_before,
                "Explore demo fleet must flip the shell's demo mode, not silently no-op"
            );
        });
        wizard.read_with(cx, |wizard, _| {
            assert!(
                !wizard.demo_requested,
                "the one-shot demo request must be consumed, not left set"
            );
        });
    }

    /// `AppShell::new_demo`'s startup load resolves on the shared tokio
    /// runtime (a real OS thread pool outside gpui's deterministic test
    /// executor), so poll with bounded, short real sleeps rather than
    /// assuming a single `run_until_parked()` lands the result.
    ///
    /// The closure takes a fresh `&mut VisualTestContext` reborrow on each
    /// tick, so it can call `debug_bounds` (a `VisualTestContext`-only
    /// method) or read entity state without holding a conflicting borrow
    /// on `cx` itself for the whole loop.
    fn wait_until(
        cx: &mut gpui::VisualTestContext,
        label: &str,
        mut condition: impl FnMut(&mut gpui::VisualTestContext) -> bool,
    ) {
        for _ in 0..200 {
            cx.run_until_parked();
            if condition(cx) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("timed out waiting for: {label}");
    }

    /// Polls until the switched-in context has finished loading and the
    /// fleet shows its instances.
    fn wait_until_context_settled(
        cx: &mut gpui::VisualTestContext,
        shell: &gpui::Entity<AppShell>,
    ) {
        wait_until(cx, "context load to settle", |cx| {
            shell.read_with(cx, |shell, cx| {
                let vm = &shell.fleet.read(cx).view_model;
                !vm.is_loading() && !vm.instances().is_empty()
            })
        });
    }

    /// Emits a main window event the way an overlay does, then lets the
    /// shell's subscription and any spawned loads run.
    fn emit_main_window_event(
        cx: &mut gpui::VisualTestContext,
        shell: &gpui::Entity<AppShell>,
        event: MainWindowEvent,
    ) {
        let main_window = shell.read_with(cx, |shell, _| shell.main_window.clone());
        main_window.update(cx, |_, cx| cx.emit(event));
        cx.run_until_parked();
    }

    fn fleet_error(
        cx: &mut gpui::VisualTestContext,
        shell: &gpui::Entity<AppShell>,
    ) -> Option<String> {
        shell.read_with(cx, |shell, cx| {
            shell
                .fleet
                .read(cx)
                .view_model
                .error_message()
                .map(str::to_owned)
        })
    }

    fn all_regions_event(
        shell: &gpui::Entity<AppShell>,
        cx: &mut gpui::VisualTestContext,
    ) -> MainWindowEvent {
        MainWindowEvent::RegionsChanged(shell.read_with(cx, |shell, _| {
            AppShell::available_regions(shell.demo, shell.demo_scenario)
        }))
    }

    /// A region change from the main window's region overlay must drive the
    /// shell: the context updates and the fleet reloads for the selected
    /// region.
    #[gpui::test]
    fn region_change_event_drives_shell_context_and_fleet_reload(cx: &mut TestAppContext) {
        let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);
        wait_for_row(cx, "table.row.us-east-1:i-scenario-001");

        let all_regions = shell.read_with(cx, |shell, _| {
            AppShell::available_regions(shell.demo, shell.demo_scenario)
        });
        emit_main_window_event(cx, &shell, MainWindowEvent::RegionsChanged(all_regions));

        shell.read_with(cx, |shell, _| {
            assert!(
                shell.context_model.is_all(REGION),
                "the shell's region selection must track the region overlay's selection"
            );
        });

        wait_until_context_settled(cx, &shell);
    }

    /// Switching regions must clear the previously selected instance, since
    /// the fleet state is reset for the new context.
    #[gpui::test]
    fn region_change_event_clears_the_selected_instance(cx: &mut TestAppContext) {
        let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);
        wait_for_row(cx, "table.row.us-east-1:i-scenario-001");

        let row = cx
            .debug_bounds("table.row.us-east-1:i-scenario-001")
            .expect("a seeded instance row should render after the startup load");
        cx.simulate_click(row.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        assert!(
            shell.read_with(cx, |shell, cx| shell.fleet.read(cx).selected.is_some()),
            "clicking a row should select it"
        );

        let all_regions = shell.read_with(cx, |shell, _| {
            AppShell::available_regions(shell.demo, shell.demo_scenario)
        });
        emit_main_window_event(cx, &shell, MainWindowEvent::RegionsChanged(all_regions));

        let (is_all, selection) = shell.read_with(cx, |shell, cx| {
            (
                shell.context_model.is_all(REGION),
                shell.fleet.read(cx).selected.clone(),
            )
        });
        assert!(
            is_all,
            "the shell's region selection must track the selection"
        );
        assert!(
            selection.is_none(),
            "the old selection must not survive a context switch"
        );

        wait_until_context_settled(cx, &shell);
    }

    /// The empty-state path: a shell launched with the empty-fleet demo
    /// scenario settles with no instances and no error, and the centered
    /// empty state renders once the load completes.
    #[gpui::test]
    fn empty_demo_settles_with_no_instances_and_no_error(cx: &mut TestAppContext) {
        let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo_empty);
        wait_until(cx, "the empty load to settle", |cx| {
            shell.read_with(cx, |shell, cx| {
                !shell.fleet.read(cx).view_model.is_loading()
            }) && cx.debug_bounds("main.center-state").is_some()
        });

        shell.read_with(cx, |shell, cx| {
            let vm = &shell.fleet.read(cx).view_model;
            assert!(
                vm.instances().is_empty(),
                "the empty-fleet scenario must have no instances"
            );
            assert!(
                vm.error_message().is_none(),
                "an empty but successful load must not report an error"
            );
        });
    }

    /// The switch-profile action opens the profile overlay, and escape closes
    /// it without changing the context.
    #[gpui::test]
    fn switch_profile_action_opens_and_escape_dismisses_without_changing_context(
        cx: &mut TestAppContext,
    ) {
        let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);
        wait_for_row(cx, "table.row.us-east-1:i-scenario-001");

        let context_before = shell.read_with(cx, |shell, _| shell.context.clone());

        cx.dispatch_action(crate::app_menus::SwitchProfile);
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("main.overlay.profile").is_some(),
            "the switch-profile action should open the profile overlay"
        );

        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("main.overlay.profile").is_none(),
            "escape should close the profile overlay"
        );

        let context_after = shell.read_with(cx, |shell, _| shell.context.clone());
        assert_eq!(
            context_after, context_before,
            "dismissing the profile overlay must not change the context"
        );
    }

    /// Selecting a different profile must update the shell's context and
    /// reload the fleet for the new profile.
    #[gpui::test]
    fn pick_profile_event_drives_shell_context(cx: &mut TestAppContext) {
        let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);
        wait_for_row(cx, "table.row.us-east-1:i-scenario-001");

        emit_main_window_event(
            cx,
            &shell,
            MainWindowEvent::PickProfile("other-profile".into()),
        );

        let context_profile = shell.read_with(cx, |shell, _| shell.context.profile.clone());
        assert_eq!(
            context_profile, "other-profile",
            "the shell's context profile must match the picked profile"
        );

        // The demo service for an unknown profile returns no instances and no error.
        wait_until(cx, "profile switch load to settle", |cx| {
            shell.read_with(cx, |shell, cx| {
                let vm = &shell.fleet.read(cx).view_model;
                !vm.is_loading() && vm.instances().is_empty()
            })
        });
        assert!(
            fleet_error(cx, &shell).is_none(),
            "the demo service for an unknown profile should not produce an error"
        );
    }

    /// A region change on a failure-scenario shell must settle with the
    /// simulated load error on the fleet.
    #[gpui::test]
    fn region_change_on_failure_shell_settles_with_the_simulated_error(cx: &mut TestAppContext) {
        let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo_failure);
        wait_until(cx, "error to render after startup", |cx| {
            fleet_error(cx, &shell).is_some()
        });

        assert!(
            shell.read_with(cx, |shell, _| !shell.context_model.is_all(REGION)),
            "the test must start in a concrete region so the fleet switch is a meaningful change"
        );

        let event = all_regions_event(&shell, cx);
        emit_main_window_event(cx, &shell, event);

        assert!(
            shell.read_with(cx, |shell, _| shell.context_model.is_all(REGION)),
            "the shell's region selection must track the region overlay's selection"
        );

        wait_until(cx, "the switched failure load to settle", |cx| {
            fleet_error(cx, &shell).is_some()
        });
        let error_msg = fleet_error(cx, &shell);
        assert!(
            error_msg
                .as_deref()
                .unwrap_or("")
                .contains("Simulated demo load failure"),
            "the error message should be the simulated demo failure, got: {error_msg:?}"
        );
    }

    /// A region change with a gated service injected into `apply_context`
    /// must show the fleet loading while the load is in flight, then the
    /// simulated error once the gate is released.
    #[gpui::test]
    fn region_change_shows_loading_state_during_failure_load(cx: &mut TestAppContext) {
        let gate = Gate::new();
        let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo_failure);
        wait_until(cx, "error to render after startup", |cx| {
            fleet_error(cx, &shell).is_some()
        });

        assert!(
            shell.read_with(cx, |shell, _| !shell.context_model.is_all(REGION)),
            "the test must start in a concrete region so the fleet switch is a meaningful change"
        );

        shell.update(cx, |shell, _| {
            shell.set_test_service_override(gated_provider(
                vec![],
                Some("Simulated demo load failure".to_string()),
                gate.clone(),
            ));
        });

        let event = all_regions_event(&shell, cx);
        emit_main_window_event(cx, &shell, event);

        let (is_all, is_loading) = shell.read_with(cx, |shell, cx| {
            (
                shell.context_model.is_all(REGION),
                shell.fleet.read(cx).view_model.is_loading(),
            )
        });
        assert!(
            is_all,
            "the shell's region selection must track the region overlay's selection"
        );
        assert!(
            is_loading,
            "the fleet must report loading while the gated load is in flight"
        );
        assert!(
            fleet_error(cx, &shell).is_none(),
            "the error state must not be visible before the gate is released"
        );

        gate.open();
        wait_until(cx, "the gated failure load to settle", |cx| {
            fleet_error(cx, &shell).is_some()
        });

        let (is_loading_after, error_msg) = shell.read_with(cx, |shell, cx| {
            (
                shell.fleet.read(cx).view_model.is_loading(),
                shell
                    .fleet
                    .read(cx)
                    .view_model
                    .error_message()
                    .map(str::to_owned),
            )
        });
        assert!(
            !is_loading_after,
            "the view model must no longer be loading after the gated load settles"
        );
        assert!(
            error_msg
                .as_deref()
                .unwrap_or("")
                .contains("Simulated demo load failure"),
            "the error message should be the simulated demo failure, got: {error_msg:?}"
        );
    }

    #[gpui::test]
    fn applying_a_new_demo_context_updates_context_and_clears_old_rows(cx: &mut TestAppContext) {
        let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);
        wait_for_row(cx, "table.row.us-east-1:i-scenario-001");

        shell.update_in(cx, |shell, window, cx| {
            set_region_values(&mut shell.context_model, &["us-west-2".to_string()]);
            shell.apply_context(
                WorkspaceContext {
                    profile: "scenario-default".into(),
                },
                window,
                cx,
            );
        });

        wait_until(cx, "the new context load to settle", |cx| {
            shell.read_with(cx, |shell, cx| {
                !shell.fleet.read(cx).view_model.is_loading()
            })
        });
        shell.read_with(cx, |shell, cx| {
            assert_eq!(region_values(&shell.context_model), ["us-west-2"]);
            assert!(shell.fleet.read(cx).view_model.instances().is_empty());
        });
    }
}
