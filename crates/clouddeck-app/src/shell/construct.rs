use super::*;

impl AppShell {
    #[cfg(test)]
    pub(super) fn new_demo(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build_with_scenario(window, true, DemoFleet::Happy, cx)
    }

    #[cfg(test)]
    pub(super) fn new_demo_empty(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build_with_scenario(window, true, DemoFleet::Empty, cx)
    }

    #[cfg(test)]
    pub(super) fn new_demo_failure(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build_with_scenario(window, true, DemoFleet::Failure, cx)
    }

    #[cfg(test)]
    pub(super) fn new_demo_large(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build_with_scenario(window, true, DemoFleet::Large, cx)
    }

    #[cfg(test)]
    pub(super) fn new_demo_partial(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build_with_scenario(window, true, DemoFleet::Partial, cx)
    }

    pub(super) fn profiles(
        demo: bool,
        demo_scenario: DemoFleet,
    ) -> Result<Vec<clouddeck_core::AwsProfile>, std::io::Error> {
        if demo {
            Ok(demo_profiles(demo_scenario))
        } else {
            clouddeck_core::load_profiles_for(&clouddeck_core::aws_files_setting())
        }
    }

    pub(super) fn regions(demo: bool, demo_scenario: DemoFleet) -> Vec<String> {
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

    pub(super) fn available_regions(demo: bool, demo_scenario: DemoFleet) -> Vec<String> {
        let listed = Self::regions(demo, demo_scenario)
            .into_iter()
            .filter(|region| region != "fleet")
            .collect();
        Self::extend_demo_regions(listed, demo)
    }

    pub(super) fn extend_demo_regions(mut regions: Vec<String>, demo: bool) -> Vec<String> {
        if demo {
            for region in clouddeck_core::REGIONS.iter().copied() {
                if region != "fleet" && !regions.iter().any(|known| known == region) {
                    regions.push(region.to_string());
                }
            }
        }
        regions
    }

    pub(super) fn fleet_regions_for(model: &ContextModel) -> Vec<String> {
        if model.is_all(REGION) {
            return FLEET_REGIONS
                .iter()
                .map(|region| (*region).into())
                .collect();
        }
        region_values(model)
    }

    pub(super) fn remembered_regions_for(model: &ContextModel) -> Vec<String> {
        if model.is_all(REGION) {
            return Vec::new();
        }
        region_values(model)
    }

    pub(super) fn window_frame(window: &Window) -> WindowFrame {
        let bounds = window.bounds();
        WindowFrame {
            x: f32::from(bounds.origin.x),
            y: f32::from(bounds.origin.y),
            width: f32::from(bounds.size.width),
            height: f32::from(bounds.size.height),
        }
    }

    #[cfg(test)]
    pub(super) fn provider_for(
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

    pub(super) fn provider_for_id(
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

    pub(super) fn load_connections(
        model: &mut ContextModel,
        providers: &[std::sync::Arc<dyn Provider>],
    ) {
        let active = model.active_provider().to_string();
        let Some(provider) = providers.iter().find(|p| p.info().id == active) else {
            return;
        };
        let call = Call {
            deadline: None,
            host: &LoadHost,
        };
        model.set_connections(provider.connections(&call));
        model.set_signed_in(&active, provider_signed_in(provider.as_ref()));
    }

    pub(super) fn load_dimensions(
        model: &mut ContextModel,
        providers: &[std::sync::Arc<dyn Provider>],
    ) {
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

    pub(super) fn build_with_scenario(
        window: &mut Window,
        demo: bool,
        demo_scenario: DemoFleet,
        cx: &mut Context<Self>,
    ) -> Self {
        let ui_state_path = (!demo).then(clouddeck_core::ui_state_path);
        Self::build(window, demo, demo_scenario, ui_state_path, cx)
    }

    pub(super) fn build(
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

    pub(super) fn build_with_profiles(
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
            .then(|| WorkspaceContext::load(&saved_context_path()))
            .flatten()
            .filter(|s| s.provider != "aws" || profiles.iter().any(|p| p.name == s.profile));
        let launch_provider = settings
            .active_provider
            .clone()
            .or_else(|| saved.as_ref().map(|s| s.provider.clone()));
        let saved = saved.filter(|s| launch_provider.as_deref().is_none_or(|id| id == s.provider));
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
        let context = saved.unwrap_or_else(|| {
            let provider = launch_provider.clone().unwrap_or_else(|| "aws".into());
            let profile = if provider == "aws" {
                profiles.first().map(|p| p.name.clone()).unwrap_or_default()
            } else {
                String::new()
            };
            WorkspaceContext { provider, profile }
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
            launch_provider.as_deref(),
            "aws",
        );
        Self::load_connections(&mut context_model, &providers);
        Self::load_dimensions(&mut context_model, &providers);
        ensure_region_dimension(&mut context_model, &available);
        set_region_values(&mut context_model, &saved_regions);
        let fleet_regions = Self::fleet_regions_for(&context_model);
        let remembered_regions = Some(Self::remembered_regions_for(&context_model));
        let view: ListView = if context_model.shows_all_providers() {
            all_providers_view(
                &providers,
                demo.then_some(demo_scenario),
                request_timeout,
                &context,
            )
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
            #[cfg(test)]
            test_service_override: None,
        }
    }

    pub(super) fn new_main_window(
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
                    if result.failures.is_empty() {
                        let active = this.context_model.active_provider().to_string();
                        this.context_model.set_signed_in(&active, true);
                    }
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
                        provider: this.context_model.active_provider().to_string(),
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
                MainWindowEvent::SignIn(id) => this.sign_in(id, cx),
                MainWindowEvent::Retry(_) => this.refresh(cx),
                MainWindowEvent::OpenSettings => this.open_settings(cx),
            },
        )
        .detach();
        main_window
    }
}
