//! Linux client-side titlebar and in-window app menu.

use gpui::{
    App, ClickEvent, Context, FocusHandle, IntoElement, KeyBinding, MenuItem, MouseButton,
    MouseDownEvent, Render, RenderOnce, Window, actions, deferred, div, prelude::*, px,
};
use gpui_component::Sizable;

use crate::app_menus::app_menus;
use crate::icons::AppIcon;
use crate::tokens::{FONT_SIZE, Palette};

actions!(linux_titlebar, [CloseAppMenu]);

pub fn key_bindings() -> [KeyBinding; 1] {
    [KeyBinding::new("escape", CloseAppMenu, Some("AppMenu"))]
}

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
    focus: Option<FocusHandle>,
    return_focus: Option<FocusHandle>,
}

impl AppMenuButton {
    fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = true;
        self.return_focus = window.focused(cx);
        window.prevent_default();
        let focus = self.focus.get_or_insert_with(|| cx.focus_handle()).clone();
        focus.focus(window);
        cx.notify();
    }

    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = false;
        if let Some(focus) = self.return_focus.take() {
            focus.focus(window);
        }
        cx.notify();
    }
}

impl Render for AppMenuButton {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = crate::tokens::palette(cx);
        let was_open = self.open;
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
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                    // The panel's click-away handler has already run for this press,
                    // so decide from the state at render time rather than this.open.
                    if was_open {
                        this.dismiss(window, cx);
                    } else {
                        this.show(window, cx);
                    }
                }),
            )
            .child(AppIcon::Ellipsis.icon().small().text_color(palette.dim));

        div().relative().child(trigger).when(self.open, |root| {
            root.when_some(self.focus.clone(), |root, focus| {
                root.child(deferred(dropdown(&palette, &focus, cx)))
            })
        })
    }
}

fn dropdown(
    palette: &Palette,
    focus: &FocusHandle,
    cx: &Context<AppMenuButton>,
) -> impl IntoElement {
    let mut panel = div()
        .key_context("AppMenu")
        .track_focus(focus)
        .on_action(cx.listener(|this, _: &CloseAppMenu, window, cx| {
            this.dismiss(window, cx);
        }))
        .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, window, cx| {
            this.dismiss(window, cx);
        }))
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
                                this.dismiss(window, cx);
                                window.dispatch_action(action.boxed_clone(), cx);
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

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Entity, KeyBinding, Point, TestAppContext, VisualTestContext, point};

    actions!(linux_titlebar_tests, [HostEscape]);

    struct Host {
        menu: Entity<AppMenuButton>,
        host_escapes: usize,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .key_context("TestHost")
                .on_action(cx.listener(|this, _: &HostEscape, _, _| this.host_escapes += 1))
                .child(self.menu.clone())
        }
    }

    fn open_button(cx: &mut TestAppContext) -> (Entity<AppMenuButton>, &mut VisualTestContext) {
        cx.update(|cx| cx.bind_keys(key_bindings()));
        cx.add_window_view(|_, _| AppMenuButton::default())
    }

    fn is_open(view: &Entity<AppMenuButton>, cx: &mut VisualTestContext) -> bool {
        view.read_with(cx, |menu, _| menu.open)
    }

    fn click_trigger(cx: &mut VisualTestContext) {
        let trigger = cx.debug_bounds("linux.menu").expect("trigger renders");
        cx.simulate_click(trigger.center(), Default::default());
        cx.run_until_parked();
    }

    #[gpui::test]
    fn trigger_click_opens_then_closes_the_menu(cx: &mut TestAppContext) {
        let (view, cx) = open_button(cx);

        click_trigger(cx);
        assert!(is_open(&view, cx));
        assert!(cx.debug_bounds("linux.menu.item.0").is_some());

        click_trigger(cx);
        assert!(!is_open(&view, cx));
    }

    #[gpui::test]
    fn click_outside_the_panel_closes_the_menu(cx: &mut TestAppContext) {
        let (view, cx) = open_button(cx);
        click_trigger(cx);
        assert!(is_open(&view, cx));

        let outside: Point<gpui::Pixels> = point(px(400.0), px(300.0));
        cx.simulate_click(outside, Default::default());
        cx.run_until_parked();

        assert!(!is_open(&view, cx));
    }

    #[gpui::test]
    fn escape_closes_the_menu(cx: &mut TestAppContext) {
        let (view, cx) = open_button(cx);
        click_trigger(cx);
        assert!(is_open(&view, cx));

        cx.simulate_keystrokes("escape");
        cx.run_until_parked();

        assert!(!is_open(&view, cx));
    }

    #[gpui::test]
    fn escape_is_consumed_by_the_menu_before_the_enclosing_context(cx: &mut TestAppContext) {
        cx.update(|cx| cx.bind_keys([KeyBinding::new("escape", HostEscape, Some("TestHost"))]));
        cx.update(|cx| cx.bind_keys(key_bindings()));
        let (host, cx) = cx.add_window_view(|_, cx| Host {
            menu: cx.new(|_| AppMenuButton::default()),
            host_escapes: 0,
        });
        let menu = host.read_with(cx, |host, _| host.menu.clone());
        click_trigger(cx);
        assert!(is_open(&menu, cx));

        cx.simulate_keystrokes("escape");
        cx.run_until_parked();

        assert!(!is_open(&menu, cx));
        assert_eq!(host.read_with(cx, |host, _| host.host_escapes), 0);
    }
}
