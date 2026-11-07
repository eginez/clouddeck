//! About window: app identity, build info and the plugin list.

use std::time::Duration;

use clouddeck_core::about::{AboutInfo, PluginRow, PluginStatus, about_info};
use clouddeck_core::extension_host::{
    bundled_extensions_root, extension_state_root, extensions_root,
};
use gpui::{
    AnyElement, App, Bounds, ClickEvent, ClipboardItem, Context, FocusHandle, Global, IntoElement,
    KeyDownEvent, ParentElement, Render, SharedString, Styled, TitlebarOptions, Window,
    WindowBounds, WindowHandle, WindowOptions, div, img, point, prelude::*, px, size,
};
use gpui_component::Root;

use crate::app_providers;
use crate::tokens::{MONO_FONT, Palette, palette};

pub(crate) const WINDOW_SIZE: (f32, f32) = (440.0, 480.0);
const PLUGIN_LIST_MAX_HEIGHT: f32 = 140.0;
const TITLEBAR_INSET: f32 = 36.0;
const COPIED_DURATION: Duration = Duration::from_millis(1_800);

struct OpenAboutWindow(WindowHandle<Root>);

impl Global for OpenAboutWindow {}

/// Opens the About window with this build's info, or focuses it when already open.
pub fn open(cx: &mut App) -> Option<WindowHandle<Root>> {
    let info = about_info(
        bundled_extensions_root().as_deref(),
        &extensions_root(),
        &extension_state_root(),
        &app_providers::refusals(cx),
    )
    .ok()?;
    show(cx, info)
}

fn show(cx: &mut App, info: AboutInfo) -> Option<WindowHandle<Root>> {
    if let Some(handle) = cx.try_global::<OpenAboutWindow>().map(|open| open.0)
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return Some(handle);
    }
    let handle = open_window_for(cx, info)?;
    cx.set_global(OpenAboutWindow(handle));
    Some(handle)
}

/// Always opens a new window, bypassing the single-instance check.
pub(crate) fn open_window_for(cx: &mut App, info: AboutInfo) -> Option<WindowHandle<Root>> {
    let bounds = Bounds::centered(None, size(px(WINDOW_SIZE.0), px(WINDOW_SIZE.1)), cx);
    cx.open_window(
        WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some(SharedString::from("About CloudDeck")),
                appears_transparent: true,
                traffic_light_position: Some(point(px(14.0), px(15.0))),
            }),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            app_id: Some(crate::main_window::APP_ID.to_string()),
            ..Default::default()
        },
        move |window, cx| {
            let view = cx.new(|cx| AboutWindowView::new(info, window, cx));
            cx.new(|cx| Root::new(view, window, cx))
        },
    )
    .ok()
}

struct AboutWindowView {
    info: AboutInfo,
    focus: FocusHandle,
    copied: bool,
    copied_generation: u64,
}

impl AboutWindowView {
    fn new(info: AboutInfo, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window);
        Self {
            info,
            focus,
            copied: false,
            copied_generation: 0,
        }
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(self.info.copy_text()));
        self.copied_generation += 1;
        self.copied = true;
        let generation = self.copied_generation;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COPIED_DURATION).await;
            let _ = this.update(cx, |this, cx| {
                if this.copied_generation == generation {
                    this.copied = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

impl Render for AboutWindowView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = palette(cx);
        let copy_label = if self.copied { "Copied" } else { "Copy" };
        let title = format!("{} {}", self.info.name, self.info.version);

        div()
            .key_context("AboutWindow")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|_, event: &KeyDownEvent, window, _| {
                if event.keystroke.key == "escape" {
                    window.remove_window();
                }
            }))
            .size_full()
            .flex()
            .flex_col()
            .gap(px(12.))
            .px(px(20.))
            .pt(px(TITLEBAR_INSET))
            .pb(px(20.))
            .bg(palette.win)
            .text_color(palette.text)
            .text_size(px(13.))
            .child(
                div().flex().justify_center().child(
                    img(crate::assets::APP_ICON_PATH)
                        .size(px(64.))
                        .debug_selector(|| "about.icon".into()),
                ),
            )
            .child(
                div()
                    .debug_selector(|| "about.title".into())
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_size(px(20.))
                    .child(title),
            )
            .child(field(
                "Commit",
                self.info.commit.clone(),
                "about.commit",
                &palette,
            ))
            .child(field(
                "Version",
                self.info.version_short.clone(),
                "about.version-short",
                &palette,
            ))
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(palette.dim)
                    .child("Plugins"),
            )
            .child(plugin_table(&self.info.plugins, &palette))
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        button("about.copy", copy_label, &palette)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.copy(cx))),
                    )
                    .child(button("about.ok", "OK", &palette).on_click(
                        cx.listener(|_, _: &ClickEvent, window, _| window.remove_window()),
                    )),
            )
    }
}

fn field(
    label: &'static str,
    value: String,
    selector: &'static str,
    palette: &Palette,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(16.))
        .child(
            div()
                .w(px(72.))
                .text_size(px(12.))
                .text_color(palette.dim)
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .text_size(px(12.))
                .font_family(MONO_FONT)
                .debug_selector(move || selector.into())
                .child(value),
        )
        .into_any_element()
}

fn plugin_table(plugins: &[PluginRow], palette: &Palette) -> AnyElement {
    div()
        .id("about.plugins")
        .debug_selector(|| "about.plugins".into())
        .max_h(px(PLUGIN_LIST_MAX_HEIGHT))
        .overflow_y_scroll()
        .border_1()
        .border_color(palette.line)
        .rounded(px(6.))
        .bg(palette.surf)
        .text_size(px(12.))
        .children(
            plugins
                .iter()
                .enumerate()
                .map(|(ix, row)| plugin_row(ix, row, palette).into_any_element()),
        )
        .into_any_element()
}

fn plugin_row(ix: usize, row: &PluginRow, palette: &Palette) -> impl IntoElement {
    let state_color = match row.status {
        PluginStatus::Refused(_) => palette.err,
        PluginStatus::Disabled => palette.dim,
        PluginStatus::Ok => palette.text,
    };
    div()
        .debug_selector(move || format!("about.plugin.{ix}"))
        .flex()
        .items_center()
        .gap(px(8.))
        .px(px(8.))
        .py(px(5.))
        .border_b_1()
        .border_color(palette.line2)
        .child(
            div()
                .debug_selector(move || format!("about.plugin.{ix}.name"))
                .flex_1()
                .min_w(px(0.))
                .overflow_hidden()
                .text_ellipsis()
                .child(row.name.clone()),
        )
        .child(
            div()
                .w(px(48.))
                .overflow_hidden()
                .text_ellipsis()
                .child(row.version.clone()),
        )
        .child(
            div()
                .w(px(60.))
                .text_color(palette.dim)
                .child(row.origin.label()),
        )
        .child(
            div()
                .w(px(96.))
                .min_w(px(0.))
                .overflow_hidden()
                .text_ellipsis()
                .text_color(state_color)
                .child(row.status.label()),
        )
}

fn button(id: &'static str, label: &'static str, palette: &Palette) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .px(px(12.))
        .py(px(4.))
        .rounded(px(6.))
        .border_1()
        .border_color(palette.line)
        .bg(palette.surf)
        .text_size(px(12.))
        .text_color(palette.text)
        .child(label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clouddeck_core::about::{PluginOrigin, PluginRow, PluginStatus};
    use gpui::{Entity, TestAppContext, VisualTestContext};

    const HASH: &str = "0123456789abcdef0123456789abcdef01234567";

    fn info() -> AboutInfo {
        AboutInfo {
            name: "CloudDeck".into(),
            version: "0.2.1".into(),
            commit: HASH.into(),
            version_short: "0.2.1+stable.01234567".into(),
            version_full: format!("0.2.1+stable.{HASH}"),
            plugins: vec![PluginRow {
                id: "demo".into(),
                name: "Demo".into(),
                version: "0.2.1".into(),
                origin: PluginOrigin::Bundled,
                enabled: true,
                status: PluginStatus::Ok,
            }],
        }
    }

    fn open(cx: &mut TestAppContext) -> (Entity<AboutWindowView>, &mut VisualTestContext) {
        let handle = cx
            .update(|cx| {
                gpui_component::init(cx);
                open_window_for(cx, info())
            })
            .expect("about window opens");
        let view = cx
            .update(|cx| {
                handle
                    .update(cx, |root, _, _| {
                        root.view().clone().downcast::<AboutWindowView>().ok()
                    })
                    .ok()
                    .flatten()
            })
            .expect("about view is reachable");
        let window_cx = VisualTestContext::from_window(handle.into(), cx).into_mut();
        window_cx.run_until_parked();
        (view, window_cx)
    }

    fn click(cx: &mut VisualTestContext, selector: &'static str) {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} should be rendered"));
        cx.simulate_click(bounds.center(), Default::default());
        cx.run_until_parked();
    }

    fn window_count(cx: &mut TestAppContext) -> usize {
        cx.update(|cx| cx.windows().len())
    }

    #[gpui::test]
    fn shows_build_info_and_plugin_rows(cx: &mut TestAppContext) {
        let (_, window) = open(cx);

        assert!(window.debug_bounds("about.title").is_some());
        assert!(window.debug_bounds("about.commit").is_some());
        assert!(window.debug_bounds("about.version-short").is_some());
        assert!(window.debug_bounds("about.plugins").is_some());
        assert!(window.debug_bounds("about.plugin.0").is_some());
        assert!(window.debug_bounds("about.plugin.1").is_none());
    }

    #[gpui::test]
    fn copy_puts_the_copy_text_on_the_clipboard(cx: &mut TestAppContext) {
        let (view, window) = open(cx);

        click(window, "about.copy");

        assert_eq!(
            window.read_from_clipboard().and_then(|item| item.text()),
            Some(info().copy_text())
        );
        assert!(view.read_with(window, |view, _| view.copied));
    }

    #[gpui::test]
    fn copied_label_reverts_after_a_delay(cx: &mut TestAppContext) {
        let (view, window) = open(cx);

        click(window, "about.copy");
        window.executor().advance_clock(COPIED_DURATION);
        window.run_until_parked();

        assert!(!view.read_with(window, |view, _| view.copied));
    }

    #[gpui::test]
    fn a_second_copy_keeps_the_label_past_the_first_timer(cx: &mut TestAppContext) {
        let (view, window) = open(cx);

        click(window, "about.copy");
        window.executor().advance_clock(COPIED_DURATION / 2);
        click(window, "about.copy");
        window.executor().advance_clock(COPIED_DURATION / 2);
        window.run_until_parked();

        assert!(view.read_with(window, |view, _| view.copied));
    }

    #[gpui::test]
    fn escape_closes_the_window(cx: &mut TestAppContext) {
        let (_, window) = open(cx);

        window.simulate_keystrokes("escape");
        window.run_until_parked();

        assert_eq!(window_count(cx), 0);
    }

    #[gpui::test]
    fn ok_closes_the_window(cx: &mut TestAppContext) {
        let (_, window) = open(cx);

        click(window, "about.ok");

        assert_eq!(window_count(cx), 0);
    }

    #[gpui::test]
    fn show_reuses_the_open_window(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            show(cx, info());
            show(cx, info());
        });

        assert_eq!(window_count(cx), 1);
    }
}
