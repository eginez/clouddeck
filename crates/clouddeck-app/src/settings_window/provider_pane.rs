use super::*;

impl SettingsWindow {
    pub(super) fn provider_control(
        &self,
        extension: &str,
        def: &SettingDef,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selector = format!("settings.provider.control.{extension}.{}", def.key);
        let field = field_key(extension, &def.key);
        match &def.kind {
            SettingKind::Bool => {
                let on = matches!(
                    self.providers.setting_value(extension, &def.key),
                    Some(SettingValue::Bool(true))
                );
                let extension_owned = extension.to_owned();
                let key_owned = def.key.clone();
                switch(
                    selector,
                    on,
                    palette,
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.toggle_provider_flag(&extension_owned, &key_owned, cx)
                    }),
                )
            }
            SettingKind::Choice(_) => {
                match self.provider_selects.iter().find(|(key, _)| *key == field) {
                    Some((_, state)) => choice_field(state, selector, palette),
                    None => div().into_any_element(),
                }
            }
            SettingKind::Folder => match self.provider_inputs.iter().find(|(key, _)| *key == field)
            {
                Some((_, state)) => {
                    self.folder_control(extension, &def.key, state, selector, palette, cx)
                }
                None => div().into_any_element(),
            },
            kind => match self.provider_inputs.iter().find(|(key, _)| *key == field) {
                Some((_, state)) => text_field(state, kind, selector, palette),
                None => div().into_any_element(),
            },
        }
    }

    pub(super) fn folder_control(
        &self,
        extension: &str,
        key: &str,
        state: &Entity<InputState>,
        selector: String,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row = format!("{extension}.{key}");
        let browse_extension = extension.to_owned();
        let browse_key = key.to_owned();
        let clear_extension = extension.to_owned();
        let clear_key = key.to_owned();
        let browse = link(
            format!("settings-browse-{row}"),
            format!("settings.provider.browse.{row}"),
            "Browse",
            palette,
            cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.browse_provider_folder(&browse_extension, &browse_key, window, cx)
            }),
        );
        let clear = link(
            format!("settings-clear-{row}"),
            format!("settings.provider.clear.{row}"),
            "Clear",
            palette,
            cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.clear_provider_folder(&clear_extension, &clear_key, window, cx)
            }),
        );
        folder_field(state, selector, browse, clear)
    }

    pub(super) fn provider_setting_row(
        &self,
        extension: &str,
        def: &SettingDef,
        query: &str,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row = format!("{extension}.{}", def.key);
        let changed = self.providers.setting_changed(extension, &def.key);
        let error = self
            .setting_errors
            .get(&field_key(extension, &def.key))
            .cloned();
        let missing = matches!(def.kind, SettingKind::Folder)
            && folder_is_missing(self.providers.setting_host_text(extension, &def.key));
        let extension_owned = extension.to_owned();
        let key_owned = def.key.clone();
        let reset = changed.then(|| {
            link(
                format!("settings-reset-{row}"),
                format!("settings.provider.reset.{row}"),
                "Reset",
                palette,
                cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.reset_provider_setting(&extension_owned, &key_owned, window, cx)
                }),
            )
        });
        let control = self.provider_control(extension, def, palette, cx);
        setting_row(
            RowParts {
                scope: "settings.provider",
                item: format!("settings.provider.item.{row}"),
                row,
                def,
                query,
                changed,
                error,
                missing,
                reset,
                control,
            },
            palette,
        )
    }

    pub(super) fn provider_body(
        &self,
        category: &SettingsCategory,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut sections = Vec::new();
        let has_auth = crate::app_providers::providers(cx)
            .iter()
            .any(|provider| provider.info().id == category.extension && provider.auth().is_some());
        if has_auth {
            let id = category.extension.clone();
            sections.push(
                div()
                    .child(
                        button("settings-connect-again", "Connect again", palette).on_click(
                            cx.listener(move |_, _: &ClickEvent, _, cx| {
                                cx.emit(SettingsEvent::ConnectAgain(id.clone()));
                            }),
                        ),
                    )
                    .into_any_element(),
            );
        }
        let groups: Vec<AnyElement> = provider_groups(&category.settings)
            .into_iter()
            .map(|(group, defs)| {
                let rows = defs
                    .into_iter()
                    .map(|def| self.provider_setting_row(&category.extension, def, "", palette, cx))
                    .collect();
                section(group, rows, palette)
            })
            .collect();
        sections.extend(groups);
        sections
    }
}
