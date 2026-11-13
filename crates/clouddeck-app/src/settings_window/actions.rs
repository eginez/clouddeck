use super::*;

impl SettingsWindow {
    pub(super) fn query(&self, cx: &App) -> String {
        self.search.read(cx).value().to_string()
    }

    /// Layout-check entry point: types into the search field.
    #[cfg(test)]
    pub(crate) fn type_query(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let query = query.to_owned();
        self.search
            .update(cx, |state, cx| state.set_value(query, window, cx));
    }

    pub(super) fn open_category(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        match SETTINGS_CATEGORIES.get(index) {
            Some(name) => {
                self.category = name;
                self.provider = None;
            }
            None => {
                self.provider = self
                    .provider_categories
                    .get(index - SETTINGS_CATEGORIES.len())
                    .map(|category| category.extension.clone());
            }
        }
        self.search
            .update(cx, |state, cx| state.set_value("", window, cx));
        cx.notify();
    }

    pub(super) fn change(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut Settings)) {
        let mut next = self.settings.clone();
        edit(&mut next);
        let next = next.normalized();
        if next == self.settings {
            return;
        }
        self.settings = next;
        self.unsaved = true;
        self.schedule_save(cx);
        cx.emit(SettingsEvent::Changed(self.settings.clone()));
        cx.notify();
    }

    pub(super) fn toggle(&mut self, id: SettingId, cx: &mut Context<Self>) {
        self.change(cx, |settings| {
            if let Some(value) = flag_mut(settings, id) {
                *value = !*value;
            }
        });
    }

    pub(super) fn choose(&mut self, id: SettingId, pick: Pick, cx: &mut Context<Self>) {
        if id.is_disabled(&self.settings) {
            return;
        }
        self.change(cx, |settings| pick.apply(settings));
    }

    pub(super) fn reset(&mut self, id: SettingId, window: &mut Window, cx: &mut Context<Self>) {
        if id.is_disabled(&self.settings) {
            return;
        }
        self.change(cx, |settings| id.reset(settings));
        self.sync_controls(window, cx);
    }

    pub(super) fn restore_defaults(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(extension) = self.provider.clone() {
            self.restore_provider_defaults(&extension, window, cx);
            return;
        }
        self.change(cx, |settings| *settings = Settings::default());
        self.sync_controls(window, cx);
        self.show_note_for("Defaults restored", Some(AppIcon::Check), cx);
    }

    pub(super) fn sync_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for (id, state) in &self.selects {
            if let Some(pick) = current(*id, &self.settings) {
                state.update(cx, |state, cx| state.set_selected_value(&pick, window, cx));
            }
        }
    }

    pub(super) fn show_note(
        &mut self,
        text: impl Into<SharedString>,
        icon: Option<AppIcon>,
        cx: &mut Context<Self>,
    ) {
        self.note_generation += 1;
        self.note = Some(Note {
            text: text.into(),
            icon,
        });
        cx.notify();
    }

    pub(super) fn show_note_for(
        &mut self,
        text: impl Into<SharedString>,
        icon: Option<AppIcon>,
        cx: &mut Context<Self>,
    ) {
        self.show_note(text, icon, cx);
        let generation = self.note_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(NOTE_DURATION).await;
            this.update(cx, |this, cx| {
                if this.note_generation == generation {
                    this.note = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn edit_ui_state(
        &mut self,
        note: &'static str,
        edit: impl FnOnce(&mut UiState),
        cx: &mut Context<Self>,
    ) -> bool {
        match edit_ui_state_file(&ui_state_path(), edit) {
            Ok(()) => {
                self.show_note(note, None, cx);
                true
            }
            Err(error) => {
                self.show_note(format!("Could not save: {error}"), None, cx);
                false
            }
        }
    }

    pub(super) fn schedule_save(&mut self, cx: &mut Context<Self>) {
        self.save_generation += 1;
        let generation = self.save_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_DEBOUNCE).await;
            this.update(cx, |this, _| {
                if this.save_generation == generation {
                    this.flush_pending_save();
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn flush_pending_save(&mut self) {
        if !self.unsaved {
            return;
        }
        match self.settings.save(&self.path) {
            Ok(()) => self.unsaved = false,
            Err(error) => eprintln!("Could not save settings: {error}"),
        }
    }
}
