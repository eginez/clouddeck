use super::*;

impl SettingsWindow {
    pub(super) fn stored_data_section(&self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .mt(px(6.))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(palette.faint)
                    .child("STORED DATA"),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(palette.dim)
                    .child("CloudDeck never stores credentials. It keeps profile and region names, pinned and recent searches, and window layout in ~/.config/clouddeck. Instance data is never written to disk."),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        button("settings-reveal-data", "Reveal data folder", palette).on_click(
                            cx.listener(|_, _: &ClickEvent, _, cx| {
                                let folder = context_path();
                                if let Some(dir) = folder.parent() {
                                    cx.reveal_path(dir);
                                }
                            }),
                        ),
                    )
                    .child(
                        button("settings-clear-searches", "Clear recent searches", palette)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                if this.edit_ui_state(
                                    "Recent searches cleared",
                                    |state| state.recent_searches.clear(),
                                    cx,
                                ) {
                                    cx.emit(SettingsEvent::RecentSearchesCleared);
                                }
                            })),
                    )
                    .child(
                        button("settings-forget-scope", "Forget remembered scope", palette)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                if this.edit_ui_state(
                                    "Remembered scope forgotten",
                                    |state| state.dimension_selections.clear(),
                                    cx,
                                ) {
                                    cx.emit(SettingsEvent::RememberedScopeForgotten);
                                }
                            })),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn keymap_section(&self, palette: &Palette) -> Vec<AnyElement> {
        let info = div()
            .text_size(px(11.))
            .text_color(palette.dim)
            .mb(px(10.))
            .child("Shortcuts shown here match the tooltips and the command palette.")
            .into_any_element();
        let rows = KEYMAP
            .iter()
            .enumerate()
            .map(|(index, (action, shortcut))| keymap_row(index, action, shortcut, "", palette));
        std::iter::once(info).chain(rows).collect()
    }

    pub(super) fn search_body(
        &self,
        query: &str,
        results: &SettingsSearchResults,
        provider_hits: &[SettingsCategory],
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if results.count() + provider_match_count(provider_hits) == 0 {
            return vec![
                div()
                    .debug_selector(|| "settings.no-match".into())
                    .text_size(px(13.))
                    .text_color(palette.dim)
                    .child(settings_no_match_text(query))
                    .into_any_element(),
            ];
        }
        let mut sections: Vec<AnyElement> = results
            .groups
            .iter()
            .map(|(category, ids)| self.group_section(category, ids, palette, cx))
            .collect();
        for category in provider_hits {
            let rows = category
                .settings
                .iter()
                .map(|def| self.provider_setting_row(&category.extension, def, query, palette, cx))
                .collect();
            sections.push(section(Some(&category.name), rows, palette));
        }
        if !results.keymap_rows.is_empty() {
            sections.push(keymap_results(query, &results.keymap_rows, palette));
        }
        sections
    }

    pub(super) fn plugins_section(&self, palette: &Palette, cx: &mut Context<Self>) -> Vec<AnyElement> {
        vec![
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(
                    div()
                        .text_color(palette.dim)
                        .child("Install, enable and remove bundled, installed and dev plugins."),
                )
                .child(
                    button("plugins.open", "Open Plugins…", palette)
                        .debug_selector(|| "plugins.open".into())
                        .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                            cx.dispatch_action(&app_menus::OpenPlugins);
                        })),
                )
                .into_any_element(),
        ]
    }

    pub(super) fn about_section(&self, palette: &Palette, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let platform = if cfg!(target_os = "macos") {
            "macOS · gpui"
        } else {
            "Linux · gpui"
        };
        let version = div()
            .debug_selector(|| "about.version".into())
            .child(clouddeck_core::about::version_short());
        let icon = img(crate::assets::APP_ICON_PATH)
            .size(px(96.))
            .debug_selector(|| "about.icon".into());
        let mut rows = vec![
            div()
                .flex()
                .justify_center()
                .py(px(12.))
                .child(icon)
                .into_any_element(),
            about_row("Version", version, palette),
        ];
        rows.extend(
            [
                (
                    "Config file",
                    aws_files_setting().config.display().to_string(),
                ),
                ("Data folder", "~/.config/clouddeck".to_owned()),
                ("Platform", platform.to_owned()),
            ]
            .into_iter()
            .map(|(label, value)| about_row(label, value, palette)),
        );
        rows.push(
            div()
                .pt(px(12.))
                .flex()
                .gap(px(8.))
                .child(
                    button("about.check-updates", "Check for Updates…", palette)
                        .debug_selector(|| "about.check-updates".into())
                        .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                            cx.dispatch_action(&app_menus::CheckForUpdates);
                        })),
                )
                .child(
                    button("about.open", "About CloudDeck…", palette)
                        .debug_selector(|| "about.open".into())
                        .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                            cx.dispatch_action(&app_menus::About);
                        })),
                )
                .into_any_element(),
        );
        rows
    }
}
