use super::*;

impl Render for SettingsWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = palette(cx);
        let query = self.query(cx);
        let searching = !query.trim().is_empty();
        let results = search_settings(&query);
        let provider_hits = if searching {
            search_provider_settings(&self.provider_categories, &query)
        } else {
            Vec::new()
        };
        let visible = results.visible_categories();
        let provider_visible: Vec<&str> = provider_hits
            .iter()
            .map(|category| category.extension.as_str())
            .collect();
        let selected_provider = if searching {
            None
        } else {
            self.provider.as_deref().and_then(|extension| {
                self.provider_categories
                    .iter()
                    .find(|category| category.extension == extension)
            })
        };
        let changed_dots = changed_categories(&self.settings);
        let tree = div()
            .debug_selector(|| "settings.tree".into())
            .w(px(200.))
            .h_full()
            .flex()
            .flex_col()
            .gap(px(2.))
            .pt(px(36.))
            .px(px(10.))
            .pb(px(10.))
            .bg(palette.side)
            .border_r_1()
            .border_color(palette.line)
            .child(
                div()
                    .debug_selector(|| "settings.search".into())
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .h(px(26.))
                    .px(px(8.))
                    .mb(px(8.))
                    .rounded(px(6.))
                    .bg(palette.surf)
                    .border_1()
                    .border_color(palette.line)
                    .child(AppIcon::Search.icon().xsmall().text_color(palette.faint))
                    .child(
                        div().flex_1().min_w(px(0.)).child(
                            Input::new(&self.search)
                                .appearance(false)
                                .bordered(false)
                                .text_size(px(12.)),
                        ),
                    ),
            )
            .children(
                SETTINGS_CATEGORIES
                    .iter()
                    .enumerate()
                    .filter(|(_, name)| !searching || visible.contains(*name))
                    .map(|(index, name)| {
                        let selected =
                            !searching && self.provider.is_none() && *name == self.category;
                        self.category_button(
                            index,
                            name,
                            selected,
                            changed_dots.contains(name),
                            &palette,
                            cx,
                        )
                    }),
            )
            .children(
                self.provider_categories
                    .iter()
                    .enumerate()
                    .filter(|(_, category)| {
                        !searching || provider_visible.contains(&category.extension.as_str())
                    })
                    .map(|(offset, category)| {
                        let index = SETTINGS_CATEGORIES.len() + offset;
                        let selected = selected_provider
                            .is_some_and(|selected| selected.extension == category.extension);
                        self.category_button(
                            index,
                            &category.name,
                            selected,
                            self.provider_changed(category),
                            &palette,
                            cx,
                        )
                    }),
            );

        #[cfg(test)]
        {
            let drawn = if searching {
                results.groups.clone()
            } else if selected_provider.is_some() {
                Vec::new()
            } else {
                groups_for(self.category)
            };
            self.rendered = drawn
                .iter()
                .flat_map(|(_, ids)| ids.iter().copied())
                .collect();
            self.rendered_changes = self
                .rendered
                .iter()
                .copied()
                .filter(|id| id.is_changed(&self.settings))
                .collect();
            self.rendered_dots = (0..SETTINGS_CATEGORIES.len())
                .filter(|&index| changed_dots.contains(&SETTINGS_CATEGORIES[index]))
                .collect();
            self.rendered_search = searching;
            self.rendered_categories = (0..SETTINGS_CATEGORIES.len())
                .filter(|&index| !searching || visible.contains(&SETTINGS_CATEGORIES[index]))
                .collect();
        }
        let body = if searching {
            self.search_body(&query, &results, &provider_hits, &palette, cx)
        } else if let Some(category) = selected_provider {
            self.provider_body(category, &palette, cx)
        } else {
            match self.category {
                "Keymap" => self.keymap_section(&palette),
                "About" => self.about_section(&palette, cx),
                "Plugins" => self.plugins_section(&palette, cx),
                category => {
                    let mut sections: Vec<AnyElement> = groups_for(category)
                        .into_iter()
                        .map(|(group, ids)| self.group_section(group, &ids, &palette, cx))
                        .collect();
                    if category == "Privacy" {
                        sections.push(self.stored_data_section(&palette, cx));
                    }
                    sections
                }
            }
        };
        let (breadcrumb, heading) = if searching {
            let count = results.count() + provider_match_count(&provider_hits);
            (
                settings_search_breadcrumb(&query),
                settings_results_title(count),
            )
        } else if let Some(category) = selected_provider {
            (
                format!("Settings › {}", category.name),
                category.name.clone(),
            )
        } else {
            (
                format!("Settings › {}", self.category),
                self.category.to_owned(),
            )
        };
        let pane = div()
            .debug_selector(|| "settings.pane".into())
            .id("settings-pane")
            .flex_1()
            .overflow_y_scroll()
            .p(px(20.))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(palette.dim)
                    .mb(px(4.))
                    .when(searching, |row| {
                        row.debug_selector(|| "settings.search-crumb".into())
                    })
                    .child(breadcrumb),
            )
            .child(
                div()
                    .text_size(px(18.))
                    .text_color(palette.text)
                    .mb(px(14.))
                    .child(heading),
            )
            .children(body);

        let footer = div()
            .debug_selector(|| "settings.footer".into())
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.))
            .h(px(44.))
            .px(px(20.))
            .border_t_1()
            .border_color(palette.line)
            .child(
                div()
                    .id("settings-hint")
                    .debug_selector(|| "settings.hint".into())
                    .flex_1()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(11.))
                    .text_color(palette.dim)
                    .tooltip(move |_, cx| AnyView::from(cx.new(|_| Tooltip::new(FOOTER_HINT))))
                    .child(FOOTER_HINT),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .when_some(self.note.as_ref(), |row, note| {
                        row.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.))
                                .text_size(px(11.))
                                .text_color(palette.dim)
                                .when_some(note.icon, |note_row, icon| {
                                    note_row.child(icon.icon().xsmall().text_color(palette.accent))
                                })
                                .child(note.text.clone()),
                        )
                    })
                    .child(
                        button("settings-restore", "Restore Defaults", &palette)
                            .debug_selector(|| "settings.restore".into())
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.restore_defaults(window, cx)
                            })),
                    )
                    .child(
                        button("settings-done", "Done", &palette)
                            .debug_selector(|| "settings.done".into())
                            .on_click(cx.listener(|this, _: &ClickEvent, window, _| {
                                this.flush_pending_save();
                                window.remove_window();
                            })),
                    ),
            );

        let content = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .child(pane)
            .child(footer);

        div()
            .size_full()
            .flex()
            .bg(palette.win)
            .text_color(palette.text)
            .text_size(px(13.))
            .child(tree)
            .child(content)
    }
}
