use super::*;

impl AppShell {
    pub(super) fn sign_in(&mut self, id: &str, cx: &mut Context<Self>) {
        if !app_providers::providers(cx)
            .iter()
            .any(|provider| provider.info().id == id)
        {
            self.fleet.update(cx, |fleet, cx| {
                fleet.set_flash(format!("{id} is not installed"), cx)
            });
            return;
        }
        let connection = (self.context_model.active_provider() == id
            && !self.context.profile.is_empty())
        .then(|| ConnectionId::from(self.context.profile.as_str()));
        let shell = cx.weak_entity();
        let window_handle = self.window_handle;
        let opened = open_onboarding_window(cx, Some(id), connection, move |event, cx| {
            let _ = window_handle.update(cx, |_, window, cx| {
                let _ = shell.update(cx, |shell, cx| match event {
                    OnboardingEvent::SignedIn { provider } => {
                        if shell.context_model.active_provider() == provider {
                            let providers = app_providers::providers(cx);
                            shell.reload_active_provider(&providers, window, cx);
                        }
                        shell.refresh(cx);
                    }
                    OnboardingEvent::Open {
                        provider,
                        connection,
                    } => shell.open_connection(&provider, &connection, window, cx),
                    OnboardingEvent::ExploreDemo => shell.toggle_demo(window, cx),
                });
            });
        });
        if opened.is_none() {
            self.message = Some(format!("{id} has no sign-in to connect"));
        }
    }

    fn open_connection(
        &mut self,
        provider: &str,
        connection: &ConnectionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.context.profile = connection.as_str().to_string();
        if self.context_model.active_provider() == provider {
            let providers = app_providers::providers(cx);
            self.reload_active_provider(&providers, window, cx);
        } else {
            self.switch_provider_to(provider, &settings_path(), window, cx);
        }
    }

    pub(super) fn pick_provider(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.switch_provider_to(id, &settings_path(), window, cx);
        if self.context_model.needs_sign_in() {
            self.sign_in(id, cx);
        }
    }

    pub(super) fn switch_provider_to(
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

    pub(super) fn reload_active_provider(
        &mut self,
        providers: &[std::sync::Arc<dyn Provider>],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let regions = region_values(&self.context_model);
        Self::load_connections(&mut self.context_model, providers, &self.context);
        Self::load_dimensions(&mut self.context_model, providers);
        let available = Self::available_regions(self.demo, self.demo_scenario);
        ensure_region_dimension(&mut self.context_model, &available);
        set_region_values(&mut self.context_model, &regions);
        let connections = self.context_model.connections();
        let first_profile = connections
            .iter()
            .map(|connection| connection.id.as_str().to_string())
            .find(|id| *id == self.context.profile)
            .or_else(|| {
                connections
                    .first()
                    .map(|connection| connection.id.as_str().to_string())
            });
        reload_target(
            self.context_model.shows_all_providers(),
            self.context_model.needs_sign_in(),
            self.context.clone(),
            self.context_model.active_provider(),
            first_profile,
        )
        .map(|context| self.apply_context(context, window, cx))
        .unwrap_or_else(|| Self::sync_provider_view(&self.main_window, &self.context_model, cx));
    }

    pub(super) fn open_settings(&mut self, cx: &mut Context<Self>) {
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

    pub(super) fn apply_context(
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
                .save(&saved_context_path())
                .err()
                .map(|e| format!("Could not remember this context: {e}"))
        });
        self.schedule_ui_state_save(cx);
        cx.notify();
    }
}
