//! Command palette overlay (design/images/33-34).

use clouddeck_core::{CommandContext, CommandId, Platform, filter_commands, move_highlight};
use gpui::{
    App, ClickEvent, Context, Entity, EventEmitter, Render, ScrollHandle, SharedString, Window,
    div, prelude::*, px, white,
};
use gpui_component::input::{Enter, Escape, Input, InputEvent, InputState, MoveDown, MoveUp};

use crate::icons::AppIcon;
use crate::tokens::{FONT_SIZE, Palette, ROW_HEIGHT, STATUS_FONT_SIZE};
use crate::ui_record::{Recorded, forget_prefix, record};

const PANEL_RADIUS: f32 = 10.0;
const INPUT_ROW_HEIGHT: f32 = 44.0;
const FOOTER_HEIGHT: f32 = 24.0;
const VISIBLE_ROWS: f32 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteEvent {
    Run(CommandId),
    Dismissed,
}

pub struct CommandPalette {
    input: Entity<InputState>,
    highlighted: usize,
    context: CommandContext,
    scroll: ScrollHandle,
}

impl EventEmitter<PaletteEvent> for CommandPalette {}

impl CommandPalette {
    pub fn new(context: CommandContext, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Type a command"));
        cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.highlighted = 0;
                this.scroll.scroll_to_item(0);
                cx.notify();
            }
        })
        .detach();
        input.update(cx, |input, cx| input.focus(window, cx));
        Self {
            input,
            highlighted: 0,
            context,
            scroll: ScrollHandle::new(),
        }
    }

    pub fn visible(&self, cx: &App) -> Vec<CommandId> {
        filter_commands(&self.input.read(cx).value())
    }

    fn step_highlight(&mut self, down: bool, cx: &mut Context<Self>) {
        let len = self.visible(cx).len();
        self.highlighted = move_highlight(self.highlighted, len, down);
        self.scroll.scroll_to_item(self.highlighted);
        cx.notify();
    }

    fn run(&mut self, id: CommandId, cx: &mut Context<Self>) {
        if id.is_enabled(&self.context) {
            cx.emit(PaletteEvent::Run(id));
        }
    }

    fn run_highlighted(&mut self, cx: &mut Context<Self>) {
        let highlighted = self.visible(cx).get(self.highlighted).copied();
        if let Some(id) = highlighted {
            self.run(id, cx);
        }
    }
}

fn current_platform() -> Platform {
    if cfg!(target_os = "macos") {
        Platform::Mac
    } else {
        Platform::Linux
    }
}

fn command_row(
    ix: usize,
    id: CommandId,
    highlighted: bool,
    context: &CommandContext,
    platform: Platform,
    palette: &Palette,
    cx: &Context<CommandPalette>,
) -> impl IntoElement {
    let enabled = id.is_enabled(context);
    let text_color = if !enabled {
        palette.faint
    } else if highlighted {
        white()
    } else {
        palette.text
    };
    let shortcut_color = if highlighted { white() } else { palette.faint };
    let shortcut = id.shortcut_for(platform);

    div()
        .id(SharedString::from(format!("palette.row.{ix}")))
        .debug_selector(move || format!("palette.row.{ix}"))
        .flex()
        .items_center()
        .justify_between()
        .h(px(ROW_HEIGHT))
        .px(px(12.0))
        .mx(px(6.0))
        .rounded(px(6.0))
        .text_size(px(FONT_SIZE))
        .text_color(text_color)
        .when(highlighted, |row| row.bg(palette.accent))
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.run(id, cx)))
        .child(id.title())
        .when_some(shortcut, |row, shortcut| {
            row.child(div().text_color(shortcut_color).child(shortcut))
        })
}

impl Render for CommandPalette {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        forget_prefix(cx, "palette.");
        let palette = crate::tokens::palette(cx);
        let platform = current_platform();
        let commands = self.visible(cx);
        let highlighted = self.highlighted;
        let context = self.context;
        for (ix, id) in commands.iter().enumerate() {
            let token = if id.is_enabled(&context) {
                "text"
            } else {
                "faint"
            };
            record(
                cx,
                format!("palette.row.{ix}"),
                Recorded {
                    text: Some(id.title().to_string()),
                    fg: Some(token),
                    ..Default::default()
                },
            );
        }

        let body = if commands.is_empty() {
            div()
                .debug_selector(|| "palette.empty".into())
                .px(px(14.0))
                .py(px(12.0))
                .text_size(px(FONT_SIZE))
                .text_color(palette.dim)
                .child("No matching commands")
                .into_any_element()
        } else {
            div()
                .id("palette.list")
                .track_scroll(&self.scroll)
                .overflow_y_scroll()
                .max_h(px(ROW_HEIGHT * VISIBLE_ROWS))
                .flex()
                .flex_col()
                .py(px(4.0))
                .children(commands.iter().enumerate().map(|(ix, &id)| {
                    command_row(ix, id, ix == highlighted, &context, platform, &palette, cx)
                }))
                .into_any_element()
        };

        let input_row = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .h(px(INPUT_ROW_HEIGHT))
            .px(px(14.0))
            .border_b_1()
            .border_color(palette.line2)
            .child(AppIcon::Search.icon().text_color(palette.faint))
            .child(
                div()
                    .debug_selector(|| "palette.input".into())
                    .flex_1()
                    .child(Input::new(&self.input).appearance(false)),
            );

        let footer = div()
            .flex()
            .items_center()
            .h(px(FOOTER_HEIGHT))
            .px(px(14.0))
            .border_t_1()
            .border_color(palette.line2)
            .text_size(px(STATUS_FONT_SIZE))
            .text_color(palette.faint)
            .child("↑↓ navigate · ↵ run · esc close");

        let panel = div()
            .key_context("CommandPalette")
            .capture_action(cx.listener(|this, _: &MoveUp, _, cx| {
                this.step_highlight(false, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|this, _: &MoveDown, _, cx| {
                this.step_highlight(true, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|this, _: &Enter, _, cx| {
                this.run_highlighted(cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|_, _: &Escape, _, cx| {
                cx.emit(PaletteEvent::Dismissed);
                cx.stop_propagation();
            }))
            .w_full()
            .flex()
            .flex_col()
            .bg(palette.surf)
            .border_1()
            .border_color(palette.line)
            .rounded(px(PANEL_RADIUS))
            .shadow_lg()
            .overflow_hidden()
            .child(input_row)
            .child(body)
            .child(footer);

        panel
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::ui_record::recorded;
    use gpui::{TestAppContext, VisualTestContext};
    use gpui_component::Root;

    type Events = Rc<RefCell<Vec<PaletteEvent>>>;

    fn selection_context() -> CommandContext {
        CommandContext {
            has_selection: true,
            selection_has_private_ip: true,
            query_is_empty: true,
            ..Default::default()
        }
    }

    /// Opens the palette with `query` already in the input, so the first drawn frame
    /// is the filtered one: gpui 0.2.2 never clears `debug_bounds` between frames.
    fn open_palette<'a>(
        cx: &'a mut TestAppContext,
        query: &'static str,
    ) -> (Entity<CommandPalette>, Events, &'a mut VisualTestContext) {
        let slot = Rc::new(RefCell::new(None));
        let slot_for_window = slot.clone();
        let (_root, cx) = cx.add_window_view(move |window, cx| {
            gpui_component::init(cx);
            let palette = cx.new(|cx| CommandPalette::new(selection_context(), window, cx));
            palette.update(cx, |palette, cx| {
                palette
                    .input
                    .update(cx, |input, cx| input.set_value(query, window, cx));
            });
            *slot_for_window.borrow_mut() = Some(palette.clone());
            Root::new(palette, window, cx)
        });
        let palette = slot
            .borrow()
            .as_ref()
            .expect("palette entity should be created")
            .clone();
        let events: Events = Rc::new(RefCell::new(Vec::new()));
        let sink = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&palette, move |_, event: &PaletteEvent, _| {
                sink.borrow_mut().push(*event)
            })
            .detach();
        });
        (palette, events, cx)
    }

    #[gpui::test]
    fn renders_input_and_every_command_row(cx: &mut TestAppContext) {
        let (palette, _, cx) = open_palette(cx, "");

        assert!(cx.debug_bounds("palette.input").is_some());
        assert!(cx.debug_bounds("palette.row.15").is_some());
        assert!(cx.debug_bounds("palette.row.16").is_none());
        assert_eq!(
            palette.read_with(cx, |palette, cx| palette.visible(cx).len()),
            CommandId::ALL.len()
        );
    }

    #[gpui::test]
    fn a_query_filters_the_rendered_rows_by_words(cx: &mut TestAppContext) {
        let (palette, _, cx) = open_palette(cx, "copy");

        assert_eq!(
            palette.read_with(cx, |palette, cx| palette.visible(cx)),
            vec![
                CommandId::CopyId,
                CommandId::CopyPrivateIp,
                CommandId::CopyPublicIp
            ]
        );
        assert!(cx.debug_bounds("palette.row.2").is_some());
        assert!(cx.debug_bounds("palette.row.3").is_none());
    }

    #[gpui::test]
    fn typing_resets_the_highlight_to_the_first_row(cx: &mut TestAppContext) {
        let (palette, _, cx) = open_palette(cx, "");

        cx.simulate_keystrokes("down");
        assert_eq!(palette.read_with(cx, |palette, _| palette.highlighted), 1);

        cx.simulate_input("c");
        assert_eq!(palette.read_with(cx, |palette, _| palette.highlighted), 0);
    }

    #[gpui::test]
    fn enter_runs_the_highlighted_enabled_command(cx: &mut TestAppContext) {
        let (_, events, cx) = open_palette(cx, "");

        cx.simulate_input("copy");
        cx.simulate_keystrokes("down enter");

        assert_eq!(
            *events.borrow(),
            vec![PaletteEvent::Run(CommandId::CopyPrivateIp)]
        );
    }

    #[gpui::test]
    fn enter_on_a_disabled_command_runs_nothing(cx: &mut TestAppContext) {
        let (palette, events, cx) = open_palette(cx, "");

        cx.simulate_input("copy");
        cx.simulate_keystrokes("down down enter");

        assert_eq!(palette.read_with(cx, |palette, _| palette.highlighted), 2);
        assert!(events.borrow().is_empty());
    }

    #[gpui::test]
    fn clicking_an_enabled_row_runs_it(cx: &mut TestAppContext) {
        let (_, events, cx) = open_palette(cx, "");

        cx.simulate_input("copy");
        let row = cx
            .debug_bounds("palette.row.1")
            .expect("second row renders");
        cx.simulate_click(row.center(), Default::default());
        cx.run_until_parked();

        assert_eq!(
            *events.borrow(),
            vec![PaletteEvent::Run(CommandId::CopyPrivateIp)]
        );
    }

    #[gpui::test]
    fn no_matches_renders_the_empty_state(cx: &mut TestAppContext) {
        let (_, _, cx) = open_palette(cx, "");

        cx.simulate_input("zzz");

        assert!(cx.debug_bounds("palette.empty").is_some());
    }

    #[gpui::test]
    fn escape_dismisses_the_palette(cx: &mut TestAppContext) {
        let (_, events, cx) = open_palette(cx, "");

        cx.simulate_keystrokes("escape");

        assert_eq!(*events.borrow(), vec![PaletteEvent::Dismissed]);
    }

    #[gpui::test]
    /// Enabled rows record their title in the text token; disabled rows in the faint token.
    fn rows_record_title_and_enabled_or_disabled_token(cx: &mut TestAppContext) {
        let (_, _, cx) = open_palette(cx, "copy");

        let enabled = cx
            .update(|_, app| recorded(app, "palette.row.0"))
            .expect("enabled row records");
        assert_eq!(enabled.fg, Some("text"));
        assert!(enabled.text.is_some());
        let disabled = cx
            .update(|_, app| recorded(app, "palette.row.2"))
            .expect("disabled row records");
        assert_eq!(disabled.fg, Some("faint"));
    }
}
