//! Row context menu (design/images/36).

use clouddeck_core::{CommandContext, CommandId, Platform};
use gpui::{
    ClickEvent, Context, EventEmitter, FocusHandle, KeyDownEvent, Render, SharedString, Window,
    WindowAppearance, div, prelude::*, px, white,
};

use crate::icons::AppIcon;
use crate::tokens::{FONT_SIZE, Palette, ROW_HEIGHT};

const MENU_WIDTH: f32 = 200.0;
const MENU_RADIUS: f32 = 6.0;
const MENU_PADDING: f32 = 4.0;
const ITEM_RADIUS: f32 = 4.0;

/// Menu entries top to bottom; `None` is the separator.
pub fn context_menu_commands() -> [Option<CommandId>; 5] {
    [
        Some(CommandId::CopyId),
        Some(CommandId::CopyPrivateIp),
        Some(CommandId::CopyPublicIp),
        None,
        Some(CommandId::OpenInConsole),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowMenuEvent {
    Run(CommandId),
    Dismissed,
}

pub struct RowContextMenu {
    pub context: CommandContext,
    focus: Option<FocusHandle>,
}

impl EventEmitter<RowMenuEvent> for RowContextMenu {}

impl RowContextMenu {
    pub fn new(context: CommandContext) -> Self {
        Self {
            context,
            focus: None,
        }
    }

    fn run(&mut self, id: CommandId, cx: &mut Context<Self>) {
        if id.is_enabled(&self.context) {
            cx.emit(RowMenuEvent::Run(id));
        }
    }
}

fn item_icon(id: CommandId) -> AppIcon {
    match id {
        CommandId::OpenInConsole => AppIcon::ExternalLink,
        _ => AppIcon::Copy,
    }
}

fn menu_item(
    ix: usize,
    id: CommandId,
    context: &CommandContext,
    platform: Platform,
    palette: &Palette,
    cx: &Context<RowContextMenu>,
) -> impl IntoElement {
    let enabled = id.is_enabled(context);
    let text_color = if enabled { palette.text } else { palette.faint };
    let title = id
        .title()
        .split_once(": ")
        .map_or(id.title(), |(_, rest)| rest);
    let group = SharedString::from(format!("rowmenu.item.{ix}"));

    div()
        .id(group.clone())
        .group(group.clone())
        .debug_selector(move || format!("rowmenu.item.{ix}"))
        .flex()
        .items_center()
        .gap(px(8.0))
        .h(px(ROW_HEIGHT))
        .px(px(8.0))
        .rounded(px(ITEM_RADIUS))
        .text_size(px(FONT_SIZE))
        .text_color(text_color)
        .when(enabled, |row| {
            row.hover(|row| row.bg(palette.accent).text_color(white()))
        })
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.run(id, cx)))
        .child(item_icon(id).icon().text_color(text_color))
        .child(div().flex_1().child(title))
        .when_some(id.shortcut_for(platform), |row, shortcut| {
            row.child(
                div()
                    .text_color(palette.faint)
                    .group_hover(group, |s| s.text_color(white()))
                    .child(shortcut),
            )
        })
}

impl Render for RowContextMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let palette = Palette::for_dark(dark);
        let platform = if cfg!(target_os = "macos") {
            Platform::Mac
        } else {
            Platform::Linux
        };
        let context = self.context;
        let focus = match &self.focus {
            Some(focus) => focus.clone(),
            None => {
                let focus = cx.focus_handle();
                focus.focus(window);
                self.focus = Some(focus.clone());
                focus
            }
        };

        let mut item_ix = 0;
        let rows: Vec<gpui::AnyElement> = context_menu_commands()
            .into_iter()
            .map(|entry| match entry {
                Some(id) => {
                    let row = menu_item(item_ix, id, &context, platform, &palette, cx);
                    item_ix += 1;
                    row.into_any_element()
                }
                None => div()
                    .debug_selector(|| "rowmenu.separator".into())
                    .h(px(1.0))
                    .my(px(4.0))
                    .bg(palette.line2)
                    .into_any_element(),
            })
            .collect();

        div()
            .key_context("RowContextMenu")
            .track_focus(&focus)
            .on_key_down(cx.listener(|_, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    cx.emit(RowMenuEvent::Dismissed);
                    cx.stop_propagation();
                }
            }))
            .w(px(MENU_WIDTH))
            .p(px(MENU_PADDING))
            .flex()
            .flex_col()
            .bg(palette.surf)
            .border_1()
            .border_color(palette.line)
            .rounded(px(MENU_RADIUS))
            .shadow_lg()
            .children(rows)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use gpui::{Entity, TestAppContext, VisualTestContext};
    use gpui_component::Root;

    type Events = Rc<RefCell<Vec<RowMenuEvent>>>;

    fn private_ip_selection() -> CommandContext {
        CommandContext {
            has_selection: true,
            selection_has_private_ip: true,
            query_is_empty: true,
            ..Default::default()
        }
    }

    fn open_menu<'a>(
        cx: &'a mut TestAppContext,
        context: CommandContext,
    ) -> (Entity<RowContextMenu>, Events, &'a mut VisualTestContext) {
        let slot = Rc::new(RefCell::new(None));
        let slot_for_window = slot.clone();
        let (_root, cx) = cx.add_window_view(move |window, cx| {
            gpui_component::init(cx);
            let menu = cx.new(|_| RowContextMenu::new(context));
            *slot_for_window.borrow_mut() = Some(menu.clone());
            Root::new(menu, window, cx)
        });
        let menu = slot
            .borrow()
            .as_ref()
            .expect("menu entity should be created")
            .clone();
        let events: Events = Rc::new(RefCell::new(Vec::new()));
        let sink = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&menu, move |_, event: &RowMenuEvent, _| {
                sink.borrow_mut().push(*event)
            })
            .detach();
        });
        (menu, events, cx)
    }

    #[test]
    fn context_menu_commands_are_copy_id_and_ips_then_separator_then_console() {
        assert_eq!(
            context_menu_commands(),
            [
                Some(CommandId::CopyId),
                Some(CommandId::CopyPrivateIp),
                Some(CommandId::CopyPublicIp),
                None,
                Some(CommandId::OpenInConsole),
            ]
        );
    }

    #[gpui::test]
    fn renders_four_items_and_the_separator(cx: &mut TestAppContext) {
        let (_, _, cx) = open_menu(cx, private_ip_selection());

        assert!(cx.debug_bounds("rowmenu.item.0").is_some());
        assert!(cx.debug_bounds("rowmenu.item.1").is_some());
        assert!(cx.debug_bounds("rowmenu.item.2").is_some());
        assert!(cx.debug_bounds("rowmenu.item.3").is_some());
        assert!(cx.debug_bounds("rowmenu.item.4").is_none());
        assert!(cx.debug_bounds("rowmenu.separator").is_some());
    }

    #[gpui::test]
    fn clicking_an_enabled_item_runs_it(cx: &mut TestAppContext) {
        let (_, events, cx) = open_menu(cx, private_ip_selection());

        let item = cx
            .debug_bounds("rowmenu.item.0")
            .expect("copy id item renders");
        cx.simulate_click(item.center(), Default::default());
        cx.run_until_parked();

        assert_eq!(*events.borrow(), vec![RowMenuEvent::Run(CommandId::CopyId)]);
    }

    #[gpui::test]
    fn clicking_open_in_console_runs_it(cx: &mut TestAppContext) {
        let (_, events, cx) = open_menu(cx, private_ip_selection());

        let item = cx
            .debug_bounds("rowmenu.item.3")
            .expect("open in console item renders");
        cx.simulate_click(item.center(), Default::default());
        cx.run_until_parked();

        assert_eq!(
            *events.borrow(),
            vec![RowMenuEvent::Run(CommandId::OpenInConsole)]
        );
    }

    #[gpui::test]
    fn escape_dismisses_the_menu(cx: &mut TestAppContext) {
        let (_, events, cx) = open_menu(cx, private_ip_selection());

        cx.simulate_keystrokes("escape");

        assert_eq!(*events.borrow(), vec![RowMenuEvent::Dismissed]);
    }

    #[gpui::test]
    fn clicking_a_disabled_item_runs_nothing(cx: &mut TestAppContext) {
        let (_, events, cx) = open_menu(cx, private_ip_selection());

        let item = cx
            .debug_bounds("rowmenu.item.2")
            .expect("copy public ip item renders");
        cx.simulate_click(item.center(), Default::default());
        cx.run_until_parked();

        assert!(events.borrow().is_empty());
    }
}
