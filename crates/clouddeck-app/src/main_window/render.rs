use super::*;

impl MainWindow {
    pub(super) fn overlay_layer(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<(&'static str, AnyElement)> {
        let overlay = self.overlay.clone()?;
        let view = self.overlay_view.clone()?;
        let (selector, placed) = match overlay {
            Overlay::Profile => (
                "main.overlay.profile",
                overlay_panel("main.overlay.profile", view)
                    .absolute()
                    .left(px(OVERLAY_EDGE_INSET))
                    .bottom(px(OVERLAY_ABOVE_STATUS_BAR)),
            ),
            Overlay::Provider => (
                "main.overlay.provider",
                overlay_panel("main.overlay.provider", view)
                    .absolute()
                    .left(px(OVERLAY_EDGE_INSET))
                    .bottom(px(OVERLAY_ABOVE_STATUS_BAR)),
            ),
            Overlay::Region => (
                "main.overlay.region",
                overlay_panel("main.overlay.region", view)
                    .absolute()
                    .left(px(REGION_OVERLAY_LEFT))
                    .bottom(px(OVERLAY_ABOVE_STATUS_BAR)),
            ),
            Overlay::Palette => (
                "main.overlay.palette",
                div()
                    .absolute()
                    .top(px(PALETTE_TOP))
                    .left_0()
                    .right_0()
                    .px(px(20.0))
                    .flex()
                    .justify_center()
                    .child(
                        overlay_panel("main.overlay.palette", view)
                            .w_full()
                            .max_w(px(PALETTE_WIDTH)),
                    ),
            ),
            Overlay::RowMenu { position, .. } => (
                "main.overlay.rowmenu",
                div().absolute().top_0().left_0().child(deferred(
                    anchored()
                        .position(position)
                        .anchor(Corner::TopLeft)
                        .snap_to_window_with_margin(px(OVERLAY_EDGE_INSET))
                        .child(overlay_panel("main.overlay.rowmenu", view)),
                )),
            ),
            Overlay::Failure => (
                "main.overlay.failure",
                overlay_panel("main.overlay.failure", view)
                    .absolute()
                    .right(px(OVERLAY_EDGE_INSET))
                    .bottom(px(OVERLAY_ABOVE_STATUS_BAR)),
            ),
        };
        let catcher = div()
            .id("main.overlay.dismiss")
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .right_0()
            .occlude()
            .on_any_mouse_down(
                cx.listener(|this, _: &MouseDownEvent, window, cx| this.close_overlay(window, cx)),
            );
        let layer = div()
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .right_0()
            .child(catcher)
            .child(placed);
        Some((selector, layer.into_any_element()))
    }
}

impl Render for MainWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = crate::tokens::palette(cx);
        let viewport = window.viewport_size();
        let rules = layout_rules(f32::from(viewport.width), f32::from(viewport.height));

        if self.last_narrow != Some(rules.narrow) {
            self.last_narrow = Some(rules.narrow);
            self.fleet.update(cx, |fleet, _| {
                fleet.sidebar_open = rules.panes_open_by_default;
                fleet.inspector_open = rules.panes_open_by_default;
            });
        }
        self.sync_context_bar(cx);
        self.sync_failure_popover(window, cx);

        let center_content = self.center_state_for(cx);
        let (sidebar_open, inspector_visible) = {
            let fleet = self.fleet.read(cx);
            (
                fleet.sidebar_open,
                fleet.inspector_open && fleet.selected_instance().is_some(),
            )
        };
        let overlays = rules.panes_are_overlays;
        let scrim_visible = overlays && (sidebar_open || inspector_visible);
        #[cfg(test)]
        let center_state_visible = center_content.is_some();

        let center = match center_content {
            Some((state, text)) => {
                self.center.update(cx, |view, _| {
                    view.state = state;
                    view.text = text;
                });
                div()
                    .debug_selector(|| "main.center-state".into())
                    .size_full()
                    .child(self.center.clone())
                    .into_any_element()
            }
            None => div()
                .debug_selector(|| "main.table".into())
                .size_full()
                .child(self.table.clone())
                .into_any_element(),
        };
        let center = div()
            .debug_selector(|| "main.center".into())
            .flex_1()
            .min_w_0()
            .h_full()
            .child(center);

        let scrim = scrim_visible.then(|| {
            div()
                .id("main.scrim")
                .debug_selector(|| "main.scrim".into())
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .right_0()
                .bg(hsla(0.0, 0.0, 0.0, SCRIM_ALPHA))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.close_panes(cx)))
        });

        let mut middle = div().flex().flex_1().min_h_0().relative();
        if overlays {
            middle = middle
                .child(center)
                .children(scrim)
                .children(sidebar_open.then(|| self.sidebar_pane(true)))
                .children(inspector_visible.then(|| self.inspector_pane(true)));
        } else {
            middle = middle
                .children(sidebar_open.then(|| self.sidebar_pane(false)))
                .child(center)
                .children(inspector_visible.then(|| self.inspector_pane(false)));
        }

        #[cfg(test)]
        {
            self.rendered = RenderedPanes {
                sidebar: sidebar_open,
                inspector: inspector_visible,
                scrim: scrim_visible,
                center_state: center_state_visible,
            };
        }

        let overlay = self.overlay_layer(cx);
        #[cfg(test)]
        {
            self.rendered_overlay = overlay.as_ref().map(|(selector, _)| *selector);
        }

        div()
            .key_context("MainWindow")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::escape_pressed))
            .on_action(
                cx.listener(|this, _: &app_menus::SwitchProfile, window, cx| {
                    this.open_overlay(Overlay::Profile, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &app_menus::SwitchRegion, window, cx| {
                    this.open_overlay(Overlay::Region, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &app_menus::OpenCommandPalette, window, cx| {
                    this.open_overlay(Overlay::Palette, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &app_menus::ToggleSidebar, _, cx| this.toggle_sidebar(cx)),
            )
            .on_action(
                cx.listener(|this, _: &app_menus::ToggleInspector, _, cx| {
                    this.toggle_inspector(cx)
                }),
            )
            .on_action(cx.listener(|this, _: &app_menus::RefreshFleet, _, cx| this.refresh(cx)))
            .on_action(cx.listener(|this, _: &app_menus::CopyInstanceId, _, cx| {
                this.copy_selected(|instance| Some(native_id(instance).to_string()), cx)
            }))
            .on_action(cx.listener(|this, _: &app_menus::OpenInConsole, _, cx| {
                this.open_selected_in_console(cx)
            }))
            .on_action(cx.listener(|_, _: &app_menus::OpenSettings, _, cx| {
                cx.emit(MainWindowEvent::OpenSettings)
            }))
            .debug_selector(|| "main.root".into())
            .flex()
            .flex_col()
            .size_full()
            .bg(palette.win)
            .font_family(UI_FONT)
            .text_size(px(FONT_SIZE))
            .text_color(palette.text)
            .child(
                div()
                    .debug_selector(|| "main.toolbar".into())
                    .w_full()
                    .child(self.toolbar.clone()),
            )
            .child(middle)
            .child(
                div()
                    .debug_selector(|| "main.statusbar".into())
                    .w_full()
                    .child(self.context_bar.clone()),
            )
            .children(overlay.map(|(_, layer)| layer))
    }
}
