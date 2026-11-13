use super::*;

impl SettingsWindow {
    pub(super) fn category_button(
        &self,
        index: usize,
        name: &str,
        selected: bool,
        changed: bool,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let icon = category_icon(index);
        div()
            .id(("settings-cat", index))
            .debug_selector(move || format!("settings.cat.{index}"))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.open_category(index, window, cx)
            }))
            .flex()
            .items_center()
            .gap(px(8.))
            .h(px(26.))
            .px(px(8.))
            .rounded(px(6.))
            .text_size(px(12.))
            .when(selected, |row| row.bg(palette.sel).text_color(palette.text))
            .when(!selected, |row| row.text_color(palette.dim))
            .child(
                div()
                    .flex_none()
                    .debug_selector(move || format!("settings.cat-icon.{index}"))
                    .child(icon.icon().xsmall()),
            )
            .child(
                div()
                    .debug_selector(move || format!("settings.cat-label.{index}"))
                    .child(name.to_owned()),
            )
            .when(changed, |row| {
                row.child(
                    div()
                        .ml_auto()
                        .size(px(6.))
                        .rounded(px(3.))
                        .bg(palette.accent)
                        .debug_selector(move || format!("settings.cat-dot.{index}")),
                )
            })
            .into_any_element()
    }

    pub(super) fn switch(
        &self,
        id: SettingId,
        on: bool,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let track = div()
            .w(px(32.))
            .h(px(18.))
            .rounded(px(9.))
            .p(px(2.))
            .flex()
            .items_center()
            .bg(if on { palette.accent } else { palette.line })
            .when(on, |track| track.justify_end())
            .child(div().size(px(14.)).rounded(px(7.)).bg(white()));
        div()
            .id(id.label())
            .debug_selector(move || format!("settings.control.{id:?}"))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle(id, cx)))
            .child(track)
            .into_any_element()
    }

    pub(super) fn select_control(&self, id: SettingId, palette: &Palette) -> AnyElement {
        let Some((_, state)) = self.selects.iter().find(|(select_id, _)| *select_id == id) else {
            return div().into_any_element();
        };
        let disabled = id.is_disabled(&self.settings);
        div()
            .debug_selector(move || format!("settings.control.{id:?}"))
            .w(px(190.))
            .text_color(palette.text)
            .child(Select::new(state).menu_width(px(220.)).disabled(disabled))
            .into_any_element()
    }

    pub(super) fn control(&self, id: SettingId, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        match flag(&self.settings, id) {
            Some(on) => self.switch(id, on, palette, cx),
            None => self.select_control(id, palette),
        }
    }

    pub(super) fn setting_row(&self, id: SettingId, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let disabled = id.is_disabled(&self.settings);
        let changed = id.is_changed(&self.settings);
        let query = self.query(cx);
        div()
            .debug_selector(move || format!("settings.item.{id:?}"))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.))
            .py(px(9.))
            .border_b_1()
            .border_color(palette.line)
            .when(disabled, |row| row.opacity(0.5))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.))
                    .gap(px(2.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(div().w(px(2.)).h(px(14.)).rounded(px(1.)).when(
                                changed,
                                |bar| {
                                    bar.bg(palette.accent)
                                        .debug_selector(move || format!("settings.changed.{id:?}"))
                                },
                            ))
                            .child(div().text_size(px(13.)).child(highlighted(
                                id.label(),
                                &query,
                                palette,
                            )))
                            .when(changed, |label| {
                                label.child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "settings-reset-{}",
                                            id.label()
                                        )))
                                        .debug_selector(move || format!("settings.reset.{id:?}"))
                                        .cursor_pointer()
                                        .text_size(px(11.))
                                        .text_color(palette.accent)
                                        .on_click(cx.listener(
                                            move |this, _: &ClickEvent, window, cx| {
                                                this.reset(id, window, cx)
                                            },
                                        ))
                                        .child("Reset"),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(palette.dim)
                            .child(highlighted(id.description(), &query, palette)),
                    ),
            )
            .child(self.control(id, palette, cx))
            .into_any_element()
    }

    pub(super) fn group_section(
        &self,
        group: &'static str,
        ids: &[SettingId],
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .mb(px(18.))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(palette.faint)
                    .mb(px(4.))
                    .child(group.to_uppercase()),
            )
            .children(ids.iter().map(|id| self.setting_row(*id, palette, cx)))
            .into_any_element()
    }
}
