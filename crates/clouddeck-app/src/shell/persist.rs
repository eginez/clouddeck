use super::*;

impl AppShell {
    pub fn toggle_demo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let demo = !self.demo;
        let scenario = self.demo_scenario;
        self.save_ui_state(cx);
        *self = Self::build_with_scenario(window, demo, scenario, cx);
        cx.notify();
    }

    pub(super) fn schedule_ui_state_save(&mut self, cx: &mut Context<Self>) {
        if self.ui_state_path.is_none() {
            return;
        }
        self.ui_save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(UI_STATE_SAVE_DEBOUNCE).await;
            let _ = this.update(cx, |this, cx| this.save_ui_state(cx));
        }));
    }

    pub(super) fn save_ui_state(&mut self, cx: &mut Context<Self>) {
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

    pub(super) fn handle_settings_event(&mut self, event: &SettingsEvent, cx: &mut Context<Self>) {
        match event {
            SettingsEvent::Changed(settings) => {
                let main_window = self.main_window.clone();
                let _ = self.window_handle.update(cx, |_, window, cx| {
                    main_window.update(cx, |main_window, cx| {
                        main_window.apply_settings(settings, window, cx)
                    })
                });
            }
            SettingsEvent::ProviderSettingChanged { extension, key } => {
                if !changes_aws_profiles(extension, key.as_deref()) {
                    return;
                }
                let shell = cx.entity();
                let window_handle = self.window_handle;
                cx.defer(move |cx| {
                    let _ = window_handle.update(cx, |_, window, cx| {
                        shell.update(cx, |shell, cx| {
                            let providers = app_providers::providers(cx);
                            shell.reload_active_provider(&providers, window, cx);
                            shell.refresh(cx);
                        })
                    });
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
            SettingsEvent::ConnectAgain(id) => self.sign_in(id, cx),
        }
    }

    pub(super) fn ui_state_snapshot(&self, cx: &App) -> UiState {
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
            region_counts: fleet.region_counts.clone(),
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
}
