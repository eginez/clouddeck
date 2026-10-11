//! Linux client-side titlebar and in-window app menu.

use gpui::{
    App, ClickEvent, Context, IntoElement, MenuItem, Render, RenderOnce, Window, deferred, div,
    prelude::*, px,
};
use gpui_component::Sizable;

use crate::app_menus::app_menus;
use crate::icons::AppIcon;
use crate::tokens::{FONT_SIZE, Palette};

const CONTROL_SIZE: f32 = 28.0;
const CONTROL_GLYPH_SIZE: f32 = 13.0;
const BUTTON_RADIUS: f32 = 5.0;
const MENU_WIDTH: f32 = 220.0;
const MENU_HEADING_SIZE: f32 = 11.0;

#[derive(IntoElement)]
pub struct WindowControls;

impl RenderOnce for WindowControls {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = crate::tokens::palette(cx);
        div()
            .flex()
            .items_center()
            .child(window_control("linux.min", "—", &palette, |window| {
                window.minimize_window()
            }))
            .child(window_control("linux.max", "□", &palette, |window| {
                window.zoom_window()
            }))
            .child(window_control("linux.close", "✕", &palette, |window| {
                window.remove_window()
            }))
    }
}

fn window_control(
    key: &'static str,
    glyph: &'static str,
    palette: &Palette,
    on_click: impl Fn(&mut Window) + 'static,
) -> impl IntoElement {
    div()
        .id(key)
        .debug_selector(move || key.to_string())
        .flex()
        .items_center()
        .justify_center()
        .w(px(CONTROL_SIZE))
        .h(px(CONTROL_SIZE))
        .text_size(px(CONTROL_GLYPH_SIZE))
        .text_color(palette.dim)
        .hover(|button| button.bg(palette.hover))
        .on_click(move |_: &ClickEvent, window, _| on_click(window))
        .child(glyph)
}

#[derive(Default)]
pub struct AppMenuButton {
    open: bool,
}

impl Render for AppMenuButton {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = crate::tokens::palette(cx);
        let trigger = div()
            .id("linux.menu")
            .debug_selector(|| "linux.menu".into())
            .flex()
            .items_center()
            .justify_center()
            .w(px(CONTROL_SIZE))
            .h(px(CONTROL_SIZE))
            .rounded(px(BUTTON_RADIUS))
            .hover(|button| button.bg(palette.hover))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.open = !this.open;
                cx.notify();
            }))
            .child(AppIcon::Ellipsis.icon().small().text_color(palette.dim));

        div().relative().child(trigger).when(self.open, |root| {
            root.child(deferred(dropdown(&palette, cx)))
        })
    }
}

fn dropdown(palette: &Palette, cx: &Context<AppMenuButton>) -> impl IntoElement {
    let mut panel = div()
        .absolute()
        .top(px(CONTROL_SIZE + 4.0))
        .left(px(0.0))
        .occlude()
        .w(px(MENU_WIDTH))
        .py(px(4.0))
        .rounded(px(BUTTON_RADIUS))
        .bg(palette.panel)
        .border_1()
        .border_color(palette.line)
        .text_size(px(FONT_SIZE))
        .text_color(palette.text);

    let mut index: usize = 0;
    for menu in app_menus() {
        panel = panel.child(
            div()
                .px(px(10.0))
                .pt(px(6.0))
                .pb(px(2.0))
                .text_size(px(MENU_HEADING_SIZE))
                .text_color(palette.faint)
                .child(menu.name),
        );
        for item in menu.items {
            match item {
                MenuItem::Action { name, action, .. } => {
                    let row_index = index;
                    index += 1;
                    panel = panel.child(
                        div()
                            .id(("linux.menu.item", row_index))
                            .debug_selector(move || format!("linux.menu.item.{row_index}"))
                            .flex()
                            .items_center()
                            .px(px(10.0))
                            .h(px(26.0))
                            .hover(|row| row.bg(palette.hover))
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.open = false;
                                window.dispatch_action(action.boxed_clone(), cx);
                                cx.notify();
                            }))
                            .child(name),
                    );
                }
                MenuItem::Separator => {
                    panel = panel.child(div().h(px(1.0)).my(px(4.0)).bg(palette.line));
                }
                MenuItem::Submenu(_) | MenuItem::SystemMenu(_) => {
                    unreachable!("app_menus() holds only actions and separators")
                }
            }
        }
    }
    panel
}
