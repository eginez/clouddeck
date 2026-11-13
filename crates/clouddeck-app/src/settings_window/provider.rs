use super::*;

impl SettingsWindow {
    pub(super) fn provider_changed(&self, category: &SettingsCategory) -> bool {
        category.settings.iter().any(|def| {
            self.providers
                .setting_changed(&category.extension, &def.key)
        })
    }

    pub(super) fn edit_provider_text(
        &mut self,
        extension: &str,
        def: &SettingDef,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        match def.parse(text) {
            Ok(value) => self.store_provider_value(extension, &def.key, value, cx),
            Err(error) => self.reject_provider_value(extension, &def.key, error.0, cx),
        }
    }

    pub(super) fn edit_provider_choice(
        &mut self,
        extension: &str,
        def: &SettingDef,
        value: &str,
        cx: &mut Context<Self>,
    ) {
        self.edit_provider_text(extension, def, value, cx);
    }

    pub(super) fn browse_provider_folder(
        &mut self,
        extension: &str,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });
        let extension = extension.to_owned();
        let key = key.to_owned();
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| {
                let text = path.to_string_lossy().into_owned();
                this.store_provider_value(&extension, &key, SettingValue::Text(text), cx);
                this.sync_provider_controls(&extension, Some(&key), window, cx);
            });
        })
        .detach();
    }

    pub(super) fn clear_provider_folder(
        &mut self,
        extension: &str,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.store_provider_value(extension, key, SettingValue::Text(String::new()), cx);
        self.sync_provider_controls(extension, Some(key), window, cx);
    }

    pub(super) fn toggle_provider_flag(&mut self, extension: &str, key: &str, cx: &mut Context<Self>) {
        let on = matches!(
            self.providers.setting_value(extension, key),
            Some(SettingValue::Bool(true))
        );
        self.store_provider_value(extension, key, SettingValue::Bool(!on), cx);
    }

    pub(super) fn store_provider_value(
        &mut self,
        extension: &str,
        key: &str,
        value: SettingValue,
        cx: &mut Context<Self>,
    ) {
        let field = field_key(extension, key);
        if self.providers.setting_value(extension, key).as_ref() == Some(&value) {
            self.setting_errors.remove(&field);
        } else {
            match self.providers.set_setting(extension, key, value) {
                Ok(()) => {
                    self.setting_errors.remove(&field);
                    cx.emit(SettingsEvent::ProviderSettingChanged {
                        extension: extension.to_string(),
                        key: Some(key.to_string()),
                    });
                }
                Err(error) => {
                    self.setting_errors.insert(field, error.0);
                }
            }
        }
        cx.notify();
    }

    pub(super) fn reject_provider_value(
        &mut self,
        extension: &str,
        key: &str,
        message: String,
        cx: &mut Context<Self>,
    ) {
        self.setting_errors
            .insert(field_key(extension, key), message);
        cx.notify();
    }

    pub(super) fn reset_provider_setting(
        &mut self,
        extension: &str,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.providers.reset_setting(extension, key) {
            Ok(()) => {
                self.setting_errors.remove(&field_key(extension, key));
                cx.emit(SettingsEvent::ProviderSettingChanged {
                    extension: extension.to_string(),
                    key: Some(key.to_string()),
                });
            }
            Err(error) => {
                self.setting_errors
                    .insert(field_key(extension, key), error.0);
            }
        }
        self.sync_provider_controls(extension, Some(key), window, cx);
        cx.notify();
    }

    pub(super) fn restore_provider_defaults(
        &mut self,
        extension: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.providers.reset_extension_settings(extension) {
            Ok(()) => {
                self.setting_errors
                    .retain(|(owner, _), _| owner.as_str() != extension);
                cx.emit(SettingsEvent::ProviderSettingChanged {
                    extension: extension.to_string(),
                    key: None,
                });
            }
            Err(error) => {
                self.show_note(format!("Could not restore: {}", error.0), None, cx);
                return;
            }
        }
        self.sync_provider_controls(extension, None, window, cx);
        self.show_note_for("Defaults restored", Some(AppIcon::Check), cx);
    }

    /// Re-reads the controls of one extension, or of one of its keys, after a reset.
    /// Secrets are skipped: they have no stored value to show.
    pub(super) fn sync_provider_controls(
        &mut self,
        extension: &str,
        key: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let in_scope =
            |owner: &str, field: &str| owner == extension && key.is_none_or(|key| key == field);
        let secret = |field: &str| {
            self.provider_categories
                .iter()
                .find(|category| category.extension == extension)
                .and_then(|category| category.settings.iter().find(|def| def.key == field))
                .is_some_and(|def| def.secret)
        };
        for ((owner, field), state) in &self.provider_inputs {
            if !in_scope(owner, field) || secret(field) {
                continue;
            }
            let text = display_text(self.providers.setting_value(owner, field));
            state.update(cx, |input, cx| {
                if input.value() != text {
                    input.set_value(text, window, cx);
                }
            });
        }
        for ((owner, field), state) in &self.provider_selects {
            if !in_scope(owner, field) {
                continue;
            }
            let value = display_text(self.providers.setting_value(owner, field));
            state.update(cx, |state, cx| state.set_selected_value(&value, window, cx));
        }
    }

    /// Layout-check entry point: shows a provider's category as a sidebar click would.
    #[cfg(test)]
    pub(crate) fn show_provider(
        &mut self,
        extension: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self
            .provider_categories
            .iter()
            .position(|category| category.extension == extension)
        {
            self.open_category(SETTINGS_CATEGORIES.len() + index, window, cx);
        }
    }

    /// Layout-check entry point: types into a provider's text field.
    #[cfg(test)]
    pub(crate) fn type_provider_text(
        &mut self,
        extension: &str,
        key: &str,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field = field_key(extension, key);
        if let Some((_, input)) = self
            .provider_inputs
            .iter()
            .find(|(owner, _)| *owner == field)
        {
            let text = text.to_owned();
            input.update(cx, |input, cx| input.set_value(text, window, cx));
        }
    }
}
