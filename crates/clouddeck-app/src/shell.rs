use crate::render_state::{DEFAULT_ROW_MENU_ANCHOR, State, apply_render_state, fleet_loaded};
use crate::{
    fleet_state::FleetState,
    main_window::{MainWindow, MainWindowEvent},
    settings_window::{SettingsEvent, SettingsWindow, open_settings_window},
    theme::SurgicalTheme,
};
use clouddeck_core::{
    AwsSdkComputeService, ComputeService, EC2Instance, EC2Scenario, FLEET_REGIONS,
    FleetComputeService, InstanceListViewModel, MockComputeService, ProfileInput, RegionScope,
    Settings, UiState, WindowFrame, WorkspaceContext, settings_path,
};
use gpui::{
    AnyWindowHandle, App, Bounds, Context, Entity, IntoElement, KeyBinding, ParentElement, Render,
    Styled, Subscription, Task, Window, WindowHandle, actions, div, point, prelude::*, px, size,
};
use gpui_component::Root;
use std::{path::PathBuf, time::Duration};

actions!(clouddeck, [Refresh]);

const UI_STATE_SAVE_DEBOUNCE: Duration = Duration::from_secs(1);

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
    demo: bool,
    demo_scenario: DemoScenario,
    wizard: Option<Entity<crate::account_window::AccountWindowView>>,
    message: Option<String>,
    ui_state_path: Option<PathBuf>,
    ui_state_saved: UiState,
    ui_window_frame: Option<WindowFrame>,
    ui_save_task: Option<Task<()>>,
    window_bounds: Subscription,
    quit_observer: Subscription,
    settings_subscription: Option<Subscription>,
    applied_scope: RegionScope,
    remembered_scope: Vec<String>,
    /// Test-only: an optional compute service injected into `apply_context`
    /// so that a test can deterministically hold a picker-driven context
    /// switch load open (via `GatedComputeService`) and observe the
    /// `is_loading` / `Reloading` transition window that
    /// `MockComputeService` resolves too fast to reliably catch.
    #[cfg(test)]
    test_service_override: Option<std::sync::Arc<dyn ComputeService + Send + Sync>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DemoScenario {
    Happy,
    Empty,
    Failure,
    Partial,
    Large,
}

/// What the command line asked for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LaunchOptions {
    pub demo: bool,
    pub scenario: DemoScenario,
    /// Content size of the main window, from `--size WxH`.
    pub size: Option<(f32, f32)>,
    /// State to open the main window in, from `--render-state <Name>`.
    pub state: Option<State>,
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
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--demo" => demo = true,
            "--demo-scenario" => {
                explicit_scenario = Some(match args.next().as_deref() {
                    Some("empty") => DemoScenario::Empty,
                    Some("failure") => DemoScenario::Failure,
                    Some("partial") => DemoScenario::Partial,
                    Some("large") => DemoScenario::Large,
                    _ => DemoScenario::Happy,
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
        .unwrap_or(DemoScenario::Happy);
    Ok(LaunchOptions {
        demo,
        scenario,
        size,
        state,
    })
}

/// Launch options from the process arguments; exits with status 2 on a usage error.
pub(crate) fn launch_options() -> LaunchOptions {
    parse_launch_options(std::env::args().skip(1)).unwrap_or_else(|message| {
        eprintln!("clouddeck: {message}");
        eprintln!("usage: clouddeck [--demo] [--demo-scenario <name>] [--size WxH] [--render-state <Name>]");
        std::process::exit(2)
    })
}

fn scenario_for(kind: DemoScenario) -> EC2Scenario {
    let fixture = match kind {
        DemoScenario::Empty => {
            include_str!("../../clouddeck-core/testdata/ec2-scenarios/empty-fleet.json")
        }
        _ => include_str!("../../clouddeck-core/testdata/ec2-scenarios/small-happy-path.json"),
    };
    clouddeck_core::parse_scenario(fixture).expect("bundled demo")
}

/// Tests never pass `--demo`, so `cargo test` runs always see real mode here.
#[cfg(test)]
fn is_real_mode() -> bool {
    !std::env::args_os().any(|arg| arg == "--demo")
}

pub(crate) fn open_main_window(
    cx: &mut App,
    demo: bool,
    demo_scenario: DemoScenario,
) -> Option<WindowHandle<Root>> {
    open_main_window_with(cx, demo, demo_scenario, None, None)
}

/// Opens the main window at `size` (content size) and, if given, drives it into `state`
/// once the fleet has loaded, then forces a redraw so the first frame needs no input.
pub(crate) fn open_main_window_with(
    cx: &mut App,
    demo: bool,
    demo_scenario: DemoScenario,
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
    demo_scenario: DemoScenario,
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
    let handle = open_main_window(cx, false, DemoScenario::Happy)?;
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
        Self::build_with_scenario(window, true, DemoScenario::Happy, cx)
    }

    #[cfg(test)]
    fn new_demo_empty(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build_with_scenario(window, true, DemoScenario::Empty, cx)
    }

    #[cfg(test)]
    fn new_demo_failure(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build_with_scenario(window, true, DemoScenario::Failure, cx)
    }

    #[cfg(test)]
    fn new_demo_large(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build_with_scenario(window, true, DemoScenario::Large, cx)
    }

    #[cfg(test)]
    fn new_demo_partial(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build_with_scenario(window, true, DemoScenario::Partial, cx)
    }

    fn profiles(
        demo: bool,
        demo_scenario: DemoScenario,
    ) -> Result<Vec<clouddeck_core::AwsProfile>, std::io::Error> {
        if demo {
            Ok(scenario_for(demo_scenario)
                .profiles
                .into_iter()
                .map(|p| clouddeck_core::AwsProfile {
                    name: p.name,
                    region: p.regions.first().map(|r| r.name.clone()),
                })
                .collect())
        } else {
            clouddeck_core::load_profiles()
        }
    }

    fn regions(demo: bool, demo_scenario: DemoScenario) -> Vec<String> {
        if !demo {
            return clouddeck_core::REGIONS
                .iter()
                .map(|region| (*region).into())
                .collect();
        }
        let partial_regions: Vec<String> = if demo_scenario == DemoScenario::Partial {
            FLEET_REGIONS
                .iter()
                .map(|region| (*region).into())
                .collect()
        } else {
            Vec::new()
        };
        let mut regions = vec!["fleet".to_string()];
        for region in scenario_for(demo_scenario)
            .profiles
            .into_iter()
            .flat_map(|profile| profile.regions.into_iter().map(|region| region.name))
            .chain(partial_regions)
        {
            if !regions.iter().any(|known| known == &region) {
                regions.push(region);
            }
        }
        regions
    }

    fn available_regions(demo: bool, demo_scenario: DemoScenario) -> Vec<String> {
        Self::regions(demo, demo_scenario)
            .into_iter()
            .filter(|region| region != "fleet")
            .collect()
    }

    fn fleet_regions(scope: &RegionScope, available: &[String]) -> Vec<String> {
        if scope.is_all(available) {
            return FLEET_REGIONS
                .iter()
                .map(|region| (*region).into())
                .collect();
        }
        scope.selected().to_vec()
    }

    fn region_for_scope(scope: &RegionScope, available: &[String]) -> String {
        match scope.selected() {
            [only] if !scope.is_all(available) => only.clone(),
            _ => "fleet".into(),
        }
    }

    fn scope_for_region(region: &str, scope: RegionScope, available: &[String]) -> RegionScope {
        if region != "fleet" {
            return RegionScope::from_selected(&[region.to_string()], available);
        }
        if scope.selected().len() == 1 {
            return RegionScope::all(available);
        }
        scope
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

    fn instances_for(data: &EC2Scenario, profile: &str, region: &str) -> Vec<EC2Instance> {
        data.profiles
            .iter()
            .find(|candidate| candidate.name == profile)
            .map(|candidate| {
                candidate
                    .regions
                    .iter()
                    .filter(|candidate| region == "fleet" || candidate.name == region)
                    .flat_map(|candidate| {
                        candidate.instances.iter().cloned().map(|mut instance| {
                            instance.region = Some(candidate.name.clone());
                            instance
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn large_instances(mut instances: Vec<EC2Instance>) -> Vec<EC2Instance> {
        let seed = instances.clone();
        if seed.is_empty() {
            return instances;
        }
        for index in 4..=120 {
            let mut instance = seed[(index - 4) % seed.len()].clone();
            instance.instance_id = format!("i-demo-{index:03}");
            if let Some(name) = instance.tags.iter_mut().find(|tag| tag.key == "Name") {
                name.value = format!("demo-{index}");
            }
            instances.push(instance);
        }
        instances
    }

    fn demo_service(
        context: &WorkspaceContext,
        demo_scenario: DemoScenario,
        fleet_regions: &[String],
    ) -> Box<dyn ComputeService + Send + Sync> {
        let data = scenario_for(demo_scenario);
        if demo_scenario == DemoScenario::Failure {
            return Box::new(MockComputeService::new(
                vec![],
                Some(std::io::Error::other("Simulated demo load failure").into()),
            ));
        }
        if context.region == "fleet" && demo_scenario == DemoScenario::Partial {
            let profile = context.profile.clone();
            return Box::new(FleetComputeService::new(
                fleet_regions.to_vec(),
                move |region| {
                    if region == "us-east-1" {
                        Box::new(MockComputeService::new(
                            Self::instances_for(&data, &profile, region),
                            None,
                        ))
                    } else {
                        Box::new(MockComputeService::new(
                            vec![],
                            Some(
                                std::io::Error::other(format!("Simulated failure in {region}"))
                                    .into(),
                            ),
                        ))
                    }
                },
            ));
        }
        let mut instances = Self::instances_for(&data, &context.profile, &context.region);
        if demo_scenario == DemoScenario::Large {
            instances = Self::large_instances(instances);
        }
        Box::new(MockComputeService::new(instances, None))
    }

    fn service(
        context: &WorkspaceContext,
        demo: bool,
        demo_scenario: DemoScenario,
        fleet_regions: &[String],
    ) -> Box<dyn ComputeService + Send + Sync> {
        if demo {
            return Self::demo_service(context, demo_scenario, fleet_regions);
        }
        if context.profile.is_empty() {
            return Box::new(MockComputeService::new(vec![], Some(std::io::Error::other("No AWS profile selected. Use “Connect to AWS” to sign in and choose a profile, or explore the demo without credentials.").into())));
        }
        if context.region == "fleet" {
            let profile = context.profile.clone();
            Box::new(FleetComputeService::new(
                fleet_regions.to_vec(),
                move |region| Box::new(AwsSdkComputeService::new(profile.clone(), region)),
            ))
        } else {
            Box::new(AwsSdkComputeService::new(
                context.profile.clone(),
                context.region.clone(),
            ))
        }
    }

    fn build_with_scenario(
        window: &mut Window,
        demo: bool,
        demo_scenario: DemoScenario,
        cx: &mut Context<Self>,
    ) -> Self {
        let ui_state_path = (!demo).then(clouddeck_core::ui_state_path);
        Self::build(window, demo, demo_scenario, ui_state_path, cx)
    }

    fn build(
        window: &mut Window,
        demo: bool,
        demo_scenario: DemoScenario,
        ui_state_path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Self {
        let loaded = Self::profiles(demo, demo_scenario);
        Self::build_with_profiles(window, demo, demo_scenario, loaded, ui_state_path, cx)
    }

    fn build_with_profiles(
        window: &mut Window,
        demo: bool,
        demo_scenario: DemoScenario,
        loaded: Result<Vec<clouddeck_core::AwsProfile>, std::io::Error>,
        ui_state_path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Self {
        let message = loaded
            .as_ref()
            .err()
            .map(|e| format!("Could not read AWS profiles: {e}"));
        let profiles = loaded.unwrap_or_default();
        let saved = if demo {
            None
        } else {
            WorkspaceContext::load(&clouddeck_core::context_path())
        };
        let (ui_state, ui_state_path, ui_state_message) = match ui_state_path {
            Some(path) => match UiState::try_load(&path) {
                Ok(state) => (state, Some(path), None),
                Err(error) => {
                    eprintln!("Could not read UI state; leaving it unchanged: {error}");
                    (
                        UiState::default(),
                        None,
                        Some(format!(
                            "Could not read UI state, so it will not be saved this session: {error}"
                        )),
                    )
                }
            },
            None => (UiState::default(), None, None),
        };
        let message = message.or(ui_state_message);
        let available = Self::available_regions(demo, demo_scenario);
        let mut context = saved
            .filter(|s| profiles.iter().any(|p| p.name == s.profile))
            .unwrap_or_else(|| WorkspaceContext {
                profile: profiles.first().map(|p| p.name.clone()).unwrap_or_default(),
                region: if demo && demo_scenario == DemoScenario::Partial {
                    "fleet".into()
                } else {
                    profiles
                        .first()
                        .and_then(|p| p.region.clone())
                        .unwrap_or_else(|| "us-east-1".into())
                },
            });
        let restored = RegionScope::from_selected(&ui_state.selected_regions, &available);
        let scope = if restored.is_all(&available) {
            Self::scope_for_region(&context.region, restored, &available)
        } else {
            context.region = Self::region_for_scope(&restored, &available);
            restored
        };
        let fleet_regions = Self::fleet_regions(&scope, &available);
        let view_model = InstanceListViewModel::new(Self::service(
            &context,
            demo,
            demo_scenario,
            &fleet_regions,
        ));
        let mut fleet_state = FleetState::new(view_model, context.profile.clone(), demo);
        fleet_state.pinned_searches = ui_state.pinned_searches.clone();
        fleet_state.recent_searches = ui_state.recent_searches.clone();
        fleet_state.recent_instances = ui_state.recent_instances.clone();
        fleet_state.sidebar_open = ui_state.sidebar_open;
        fleet_state.inspector_open = ui_state.inspector_open;
        let fleet = cx.new(|_| fleet_state);
        let profile_inputs: Vec<ProfileInput> = profiles
            .iter()
            .map(|p| ProfileInput {
                name: p.name.clone(),
                account_id: None,
                region: p.region.clone(),
            })
            .collect();
        let remembered_scope = Self::remembered_regions(&scope, &available);
        let applied_scope = scope.clone();
        let main_window = Self::new_main_window(
            fleet.clone(),
            profile_inputs,
            demo,
            demo_scenario,
            scope,
            window,
            cx,
        );
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
            true
        });
        fleet.update(cx, |fleet, cx| fleet.refresh(cx));
        Self {
            fleet,
            main_window,
            window_handle: window.window_handle(),
            context,
            demo,
            demo_scenario,
            message,
            ui_state_saved: ui_state.clone(),
            ui_window_frame: ui_state.window,
            ui_state_path,
            ui_save_task: None,
            window_bounds,
            quit_observer,
            settings_subscription: None,
            applied_scope,
            remembered_scope,
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

    fn remembered_regions(scope: &RegionScope, available: &[String]) -> Vec<String> {
        if scope.is_all(available) {
            Vec::new()
        } else {
            scope.selected().to_vec()
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
                self.ui_state_saved.selected_regions.clear();
                self.remembered_scope.clear();
            }
        }
    }

    fn ui_state_snapshot(&self, cx: &App) -> UiState {
        let fleet = self.fleet.read(cx);
        UiState {
            selected_regions: self.remembered_scope.clone(),
            pinned_searches: fleet.pinned_searches.clone(),
            recent_searches: fleet.recent_searches.clone(),
            recent_instances: fleet.recent_instances.clone(),
            sidebar_open: fleet.sidebar_open,
            inspector_open: fleet.inspector_open,
            sidebar_width: self.ui_state_saved.sidebar_width,
            inspector_width: self.ui_state_saved.inspector_width,
            window: self.ui_window_frame.clone(),
        }
        .normalized()
    }

    /// Test-only: install a compute service that will be used (instead of
    /// the scenario-derived one) by the *next* `apply_context` call, so a
    /// test can deterministically observe the loading window of a
    /// picker-driven context switch. Must be called before the trigger
    /// that causes `apply_context` to fire.
    #[cfg(test)]
    pub fn set_test_service_override(
        &mut self,
        service: std::sync::Arc<dyn ComputeService + Send + Sync>,
    ) {
        self.test_service_override = Some(service);
    }

    fn new_main_window(
        fleet: Entity<FleetState>,
        profiles: Vec<ProfileInput>,
        demo: bool,
        demo_scenario: DemoScenario,
        scope: RegionScope,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<MainWindow> {
        let regions = Self::available_regions(demo, demo_scenario);
        let main_window = cx.new(|cx| MainWindow::new(fleet.clone(), regions, scope, window, cx));
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
        cx.observe(&fleet, |this: &mut Self, _, cx| {
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
                        region: this.context.region.clone(),
                    };
                    if next != this.context {
                        this.apply_context(next, window, cx);
                    }
                }
                MainWindowEvent::RegionsChanged(regions) => {
                    let region = match regions.as_slice() {
                        [only] => only.clone(),
                        _ => "fleet".into(),
                    };
                    let profile = this.context.profile.clone();
                    this.apply_context(WorkspaceContext { profile, region }, window, cx);
                }
                MainWindowEvent::ConnectToAws => this.open_setup(cx),
                MainWindowEvent::Retry(_) => this.refresh(cx),
                MainWindowEvent::OpenSettings => this.open_settings(cx),
            },
        )
        .detach();
        main_window
    }

    fn open_settings(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = open_settings_window(cx, Settings::load(&settings_path())) else {
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
        let (profiles, scope) = {
            let main_window = self.main_window.read(cx);
            (main_window.profiles.clone(), main_window.scope.clone())
        };
        let available = Self::available_regions(self.demo, self.demo_scenario);
        let scope = Self::scope_for_region(&self.context.region, scope, &available);
        let fleet_regions = Self::fleet_regions(&scope, &available);
        if scope != self.applied_scope {
            self.remembered_scope = Self::remembered_regions(&scope, &available);
        }
        self.applied_scope = scope.clone();
        #[cfg(test)]
        let service: Box<dyn ComputeService + Send + Sync> = self
            .test_service_override
            .as_ref()
            .map(|s| {
                // Wrap the shared `Arc<dyn>` in `ArcComputeService` so each
                // context switch gets its own `Box<dyn>` handle over the same
                // underlying service.
                Box::new(crate::test_support::ArcComputeService::new(
                    std::sync::Arc::clone(s),
                )) as _
            })
            .unwrap_or_else(|| {
                Self::service(&self.context, self.demo, self.demo_scenario, &fleet_regions)
            });
        #[cfg(not(test))]
        let service: Box<dyn ComputeService + Send + Sync> =
            Self::service(&self.context, self.demo, self.demo_scenario, &fleet_regions);
        let vm = InstanceListViewModel::new(service);
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
        // A fresh fleet and window, not an in-place reset: the previous load
        // holds only a weak handle to the old fleet, so it cannot write into
        // the new context once that entity is dropped.
        let fleet = cx.new(|_| {
            let mut fleet = FleetState::new(vm, profile, demo);
            fleet.pinned_searches = pinned;
            fleet.recent_searches = recent_searches;
            fleet.recent_instances = recent_instances;
            fleet.sidebar_open = sidebar_open;
            fleet.inspector_open = inspector_open;
            fleet
        });
        self.main_window = Self::new_main_window(
            fleet.clone(),
            profiles,
            demo,
            self.demo_scenario,
            scope,
            window,
            cx,
        );
        self.fleet = fleet;
        self.fleet.update(cx, |fleet, cx| fleet.refresh(cx));
        if !self.demo {
            self.message = self
                .context
                .save(&clouddeck_core::context_path())
                .err()
                .map(|e| format!("Could not remember this context: {e}"));
        }
        self.schedule_ui_state_save(cx);
        cx.notify();
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.fleet.update(cx, |fleet, cx| fleet.refresh(cx));
    }

    fn open_setup(&mut self, cx: &mut Context<Self>) {
        use crate::account_window::{AccountWindowMode, AccountWindowView};
        let profiles = match Self::profiles(self.demo, self.demo_scenario) {
            Ok(profiles) => profiles,
            Err(error) => {
                self.message = Some(format!("Could not read AWS profiles: {error}"));
                vec![]
            }
        };
        let profile = self.context.profile.clone();
        let region = self.context.region.clone();
        let mode = if profile.is_empty() {
            AccountWindowMode::SetupRequired
        } else {
            AccountWindowMode::ManageContext
        };
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
                            region: wizard.read(cx).selected_region.clone(),
                        };
                        let _ = window_handle.update(cx, |_, window, cx| {
                            let _ = shell.update(cx, |shell, cx| {
                                if next != shell.context {
                                    shell.apply_context(next, window, cx);
                                }
                            });
                        });
                    }
                })
                .detach();
                wizard
            },
        );
        match result {
            Ok(handle) => {
                self.wizard = handle.update(cx, |_, _, cx| cx.entity()).ok();
            }
            Err(error) => {
                self.message = Some(format!("Could not open setup: {error}"));
            }
        }
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
mod tests {
    use super::*;
    use crate::test_support::GatedComputeService;
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
        assert_eq!(options.scenario, DemoScenario::Empty);
        assert_eq!(options.size, Some((800.0, 600.0)));
    }

    #[test]
    fn explicit_demo_scenario_beats_the_state_scenario() {
        let options = parse(&["--demo-scenario", "large", "--render-state", "MainEmpty"]).unwrap();
        assert_eq!(options.scenario, DemoScenario::Large);
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
        let (_view, cx) = add_rooted_shell(cx, |window, cx| AppShell::new_demo(window, cx));
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
        let (_view, cx) = add_rooted_shell(cx, |window, cx| AppShell::new_demo_large(window, cx));
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
        let gate = std::sync::Arc::new(tokio::sync::Notify::new());
        let (shell, cx) = add_rooted_shell(cx, |window, cx| AppShell::new_demo(window, cx));
        wait_for_row(cx, "table.row.us-east-1:i-scenario-001");

        shell.update(cx, |shell, _| {
            shell.set_test_service_override(std::sync::Arc::new(GatedComputeService {
                gate: gate.clone(),
                instances: vec![],
                error: None,
            }));
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

        gate.notify_one();
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
        let (_view, cx) = add_rooted_shell(cx, |window, cx| AppShell::new_demo(window, cx));

        assert!(
            cx.debug_bounds("app-shell").is_some(),
            "app-shell root should render"
        );
    }

    #[gpui::test]
    fn renders_main_window_regions(cx: &mut TestAppContext) {
        let (_view, cx) = add_rooted_shell(cx, |window, cx| AppShell::new_demo(window, cx));
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
        let (_view, cx) = add_rooted_shell(cx, |window, cx| AppShell::new_demo(window, cx));
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
        let gate = std::sync::Arc::new(tokio::sync::Notify::new());
        let service = GatedComputeService {
            gate: gate.clone(),
            instances: vec![EC2Instance {
                instance_id: "i-gated".to_string(),
                region: Some("us-east-1".to_string()),
                instance_type: None,
                state: None,
                launch_time: None,
                public_ip_address: None,
                private_ip_address: None,
                vpc_id: None,
                subnet_id: None,
                security_groups: vec![],
                tags: vec![],
                availability_zone: None,
                image_id: None,
                key_name: None,
                architecture: None,
                state_transition_reason: None,
                iam_instance_profile_arn: None,
                private_dns_name: None,
                public_dns_name: None,
            }],
            error: None,
        };
        let view_model = InstanceListViewModel::new(Box::new(service));
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

        gate.notify_one();
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
                .any(|instance| instance.instance_id == "i-gated")),
            "the gated instance should be populated once the async load completes"
        );
    }

    #[gpui::test]
    fn main_center_and_status_bar_are_docked_in_the_window(cx: &mut TestAppContext) {
        let (_view, cx) = add_rooted_shell(cx, |window, cx| AppShell::new_demo(window, cx));
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
            AppShell::regions(true, DemoScenario::Happy),
            vec!["fleet".to_string(), "us-east-1".to_string()]
        );
        assert!(
            AppShell::regions(true, DemoScenario::Happy)
                .iter()
                .all(|region| region == "fleet" || region == "us-east-1")
        );
    }

    #[test]
    fn demo_failure_and_partial_modes_are_observable_without_aws() {
        let context = WorkspaceContext {
            profile: "scenario-default".into(),
            region: "fleet".into(),
        };

        let fleet_regions: Vec<String> = FLEET_REGIONS.iter().map(|r| (*r).into()).collect();
        let failure = AppShell::demo_service(&context, DemoScenario::Failure, &fleet_regions);
        let failure = crate::runtime::runtime()
            .block_on(failure.list_instances(None))
            .expect_err("failure scenario should fail visibly");
        assert!(failure.to_string().contains("Simulated demo load failure"));

        let partial = AppShell::demo_service(&context, DemoScenario::Partial, &fleet_regions);
        let partial = crate::runtime::runtime()
            .block_on(partial.list_instances(None))
            .expect("partial scenario should retain successful regions");
        assert_eq!(partial.instances.len(), 3);
        assert_eq!(partial.partial_failures.len(), 3);
    }

    #[test]
    fn large_demo_mode_has_enough_rows_to_exercise_scrolling() {
        let context = WorkspaceContext {
            profile: "scenario-default".into(),
            region: "us-east-1".into(),
        };
        let service = AppShell::demo_service(&context, DemoScenario::Large, &[]);
        let result = crate::runtime::runtime()
            .block_on(service.list_instances(None))
            .expect("large demo should load");
        assert_eq!(result.instances.len(), 120);
    }

    #[gpui::test]
    fn opening_setup_creates_the_wizard_and_finishing_it_updates_the_context(
        cx: &mut TestAppContext,
    ) {
        use crate::account_window::AccountConnectionState;

        let (shell, cx) = add_rooted_shell(cx, |window, cx| AppShell::new_demo(window, cx));

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
            assert_eq!(shell.context.region, "us-west-2");
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
                    DemoScenario::Happy,
                    Ok(Vec::new()),
                    Some(build_path),
                    cx,
                )
            } else {
                AppShell::build_with_scenario(window, true, DemoScenario::Empty, cx)
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
            selected_regions: vec!["us-east-1".into(), "eu-west-1".into()],
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
            AppShell::build(window, true, DemoScenario::Partial, Some(build_path), cx)
        });

        shell.read_with(cx, |shell, cx| {
            let fleet = shell.fleet.read(cx);
            assert_eq!(fleet.pinned_searches, vec!["state:stopped".to_string()]);
            assert_eq!(fleet.recent_searches, vec!["owner:esteban".to_string()]);
            assert_eq!(fleet.recent_instances, vec!["us-east-1:i-1".to_string()]);
            assert!(!fleet.sidebar_open);
            assert!(!fleet.inspector_open);
            assert_eq!(
                shell.main_window.read(cx).scope.selected(),
                ["us-east-1".to_string(), "eu-west-1".to_string()]
            );
            assert_eq!(shell.context.region, "fleet");
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
            AppShell::build(window, true, DemoScenario::Happy, Some(build_path), cx)
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

    #[gpui::test]
    fn pending_ui_state_is_flushed_on_quit(cx: &mut TestAppContext) {
        let path = temp_ui_state_path("quit-flush");
        let build_path = path.clone();
        let (shell, cx) = add_rooted_shell(cx, move |window, cx| {
            AppShell::build(window, true, DemoScenario::Happy, Some(build_path), cx)
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
    fn corrupt_ui_state_is_not_overwritten_by_autosave(cx: &mut TestAppContext) {
        let path = temp_ui_state_path("corrupt-autosave");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"not json").unwrap();
        let build_path = path.clone();
        let (shell, cx) = add_rooted_shell(cx, move |window, cx| {
            AppShell::build(window, true, DemoScenario::Happy, Some(build_path), cx)
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
            selected_regions: vec!["us-east-1".into(), "eu-west-1".into()],
            recent_searches: vec!["owner:esteban".into()],
            ..UiState::default()
        }
        .save(&path)
        .unwrap();
        let build_path = path.clone();
        let (shell, cx) = add_rooted_shell(cx, move |window, cx| {
            AppShell::build(window, true, DemoScenario::Partial, Some(build_path), cx)
        });

        let mut on_disk = UiState::load(&path);
        on_disk.recent_searches.clear();
        on_disk.selected_regions.clear();
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
        assert!(saved.selected_regions.is_empty());
        shell.read_with(cx, |shell, cx| {
            assert!(shell.fleet.read(cx).recent_searches.is_empty());
            assert_eq!(
                shell.main_window.read(cx).scope.selected(),
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
        let (shell, cx) = add_rooted_shell(cx, |window, cx| AppShell::new_demo_partial(window, cx));
        wait_until_context_settled(cx, &shell);

        let main_window = shell.read_with(cx, |shell, _| shell.main_window.clone());
        let available = AppShell::available_regions(true, DemoScenario::Partial);
        let selected = vec!["us-east-1".to_string(), "eu-west-1".to_string()];
        main_window.update(cx, |main_window, _| {
            main_window.scope = RegionScope::from_selected(&selected, &available);
        });
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
        let (shell, cx) = add_rooted_shell(cx, |window, cx| AppShell::new_demo(window, cx));

        emit_main_window_event(cx, &shell, MainWindowEvent::ConnectToAws);

        shell.read_with(cx, |shell, _| {
            assert!(
                shell.wizard.is_some(),
                "the ConnectToAws event should open the Connect to AWS window"
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
        let (shell, cx) = add_rooted_shell(cx, |window, cx| AppShell::new_demo(window, cx));

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

    /// A region change from the main window's region overlay must drive the
    /// shell: the context updates and the fleet reloads for the selected
    /// region.
    #[gpui::test]
    fn region_change_event_drives_shell_context_and_fleet_reload(cx: &mut TestAppContext) {
        let (shell, cx) = add_rooted_shell(cx, |window, cx| AppShell::new_demo(window, cx));
        wait_for_row(cx, "table.row.us-east-1:i-scenario-001");

        emit_main_window_event(
            cx,
            &shell,
            MainWindowEvent::RegionsChanged(vec!["fleet".into()]),
        );

        shell.read_with(cx, |shell, _| {
            assert_eq!(
                shell.context.region, "fleet",
                "the shell's context must track the region overlay's selection"
            );
        });

        wait_until_context_settled(cx, &shell);
    }

    /// Switching regions must clear the previously selected instance, since
    /// the fleet state is reset for the new context.
    #[gpui::test]
    fn region_change_event_clears_the_selected_instance(cx: &mut TestAppContext) {
        let (shell, cx) = add_rooted_shell(cx, |window, cx| AppShell::new_demo(window, cx));
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

        emit_main_window_event(
            cx,
            &shell,
            MainWindowEvent::RegionsChanged(vec!["fleet".into()]),
        );

        let (region, selection) = shell.read_with(cx, |shell, cx| {
            (
                shell.context.region.clone(),
                shell.fleet.read(cx).selected.clone(),
            )
        });
        assert_eq!(
            region, "fleet",
            "the shell's context must track the selection"
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
        let (shell, cx) = add_rooted_shell(cx, |window, cx| AppShell::new_demo_empty(window, cx));
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
        let (shell, cx) = add_rooted_shell(cx, |window, cx| AppShell::new_demo(window, cx));
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
        let (shell, cx) = add_rooted_shell(cx, |window, cx| AppShell::new_demo(window, cx));
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
        let (shell, cx) = add_rooted_shell(cx, |window, cx| AppShell::new_demo_failure(window, cx));
        wait_until(cx, "error to render after startup", |cx| {
            fleet_error(cx, &shell).is_some()
        });

        let starting_region = shell.read_with(cx, |shell, _| shell.context.region.clone());
        assert_ne!(
            starting_region, "fleet",
            "the test must start in a concrete region so the fleet switch is a meaningful change"
        );

        emit_main_window_event(
            cx,
            &shell,
            MainWindowEvent::RegionsChanged(vec!["fleet".into()]),
        );

        let region = shell.read_with(cx, |shell, _| shell.context.region.clone());
        assert_eq!(
            region, "fleet",
            "the shell's context must track the region overlay's selection"
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
        let gate = std::sync::Arc::new(tokio::sync::Notify::new());
        let (shell, cx) = add_rooted_shell(cx, |window, cx| AppShell::new_demo_failure(window, cx));
        wait_until(cx, "error to render after startup", |cx| {
            fleet_error(cx, &shell).is_some()
        });

        let starting_region = shell.read_with(cx, |shell, _| shell.context.region.clone());
        assert_ne!(
            starting_region, "fleet",
            "the test must start in a concrete region so the fleet switch is a meaningful change"
        );

        shell.update(cx, |shell, _| {
            let service = GatedComputeService {
                gate: gate.clone(),
                instances: vec![],
                error: Some(std::sync::Arc::new(std::io::Error::other(
                    "Simulated demo load failure",
                ))
                    as std::sync::Arc<dyn std::error::Error + Send + Sync>),
            };
            shell.set_test_service_override(std::sync::Arc::new(service));
        });

        emit_main_window_event(
            cx,
            &shell,
            MainWindowEvent::RegionsChanged(vec!["fleet".into()]),
        );

        let (region, is_loading) = shell.read_with(cx, |shell, cx| {
            (
                shell.context.region.clone(),
                shell.fleet.read(cx).view_model.is_loading(),
            )
        });
        assert_eq!(
            region, "fleet",
            "the shell's context must track the region overlay's selection"
        );
        assert!(
            is_loading,
            "the fleet must report loading while the gated load is in flight"
        );
        assert!(
            fleet_error(cx, &shell).is_none(),
            "the error state must not be visible before the gate is released"
        );

        gate.notify_one();
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
        let (shell, cx) = add_rooted_shell(cx, |window, cx| AppShell::new_demo(window, cx));
        wait_for_row(cx, "table.row.us-east-1:i-scenario-001");

        shell.update_in(cx, |shell, window, cx| {
            shell.apply_context(
                WorkspaceContext {
                    profile: "scenario-default".into(),
                    region: "us-west-2".into(),
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
            assert_eq!(shell.context.region, "us-west-2");
            assert!(shell.fleet.read(cx).view_model.instances().is_empty());
        });
    }
}
