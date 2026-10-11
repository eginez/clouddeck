//! Profile picker popover (design/images/30).

use clouddeck_core::{
    ProfileInput, ProfileRow, filter_profile_rows, move_highlight, profile_picker_empty_text,
    profile_rows,
};
use gpui::{
    App, ClickEvent, Context, Entity, EventEmitter, FontWeight, Render, ScrollHandle, SharedString,
    Window, WindowAppearance, div, prelude::*, px, white,
};
use gpui_component::input::{Enter, Escape, Input, InputEvent, InputState, MoveDown, MoveUp};

use crate::icons::AppIcon;
use crate::tokens::{FONT_SIZE, Palette, ROW_HEIGHT};

const PANEL_WIDTH: f32 = 300.0;
const PANEL_RADIUS: f32 = 8.0;
const PANEL_PADDING: f32 = 6.0;
const BADGE_SIZE: f32 = 24.0;
const BADGE_RADIUS: f32 = 6.0;
const NAME_FONT_SIZE: f32 = 13.0;
const SUBTITLE_FONT_SIZE: f32 = 11.0;
const BADGE_FONT_SIZE: f32 = 10.0;
const VISIBLE_ROWS: f32 = 6.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfilePickerEvent {
    Picked(String),
    ConnectToAws,
    Dismissed,
}

pub struct ProfilePicker {
    rows: Vec<ProfileRow>,
    current: String,
    filter: Entity<InputState>,
    highlighted: usize,
    scroll: ScrollHandle,
}

impl EventEmitter<ProfilePickerEvent> for ProfilePicker {}

impl ProfilePicker {
    pub fn new(
        profiles: Vec<ProfileInput>,
        current: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Search profiles"));
        cx.subscribe(&filter, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.highlighted = 0;
                this.scroll.scroll_to_item(0);
                cx.notify();
            }
        })
        .detach();
        filter.update(cx, |input, cx| input.focus(window, cx));
        Self {
            rows: profile_rows(&profiles),
            current,
            filter,
            highlighted: 0,
            scroll: ScrollHandle::new(),
        }
    }

    pub fn visible_rows(&self, cx: &App) -> Vec<ProfileRow> {
        filter_profile_rows(&self.rows, &self.filter.read(cx).value())
    }

    fn step_highlight(&mut self, down: bool, cx: &mut Context<Self>) {
        let len = self.visible_rows(cx).len();
        self.highlighted = move_highlight(self.highlighted, len, down);
        self.scroll.scroll_to_item(self.highlighted);
        cx.notify();
    }

    fn pick_highlighted(&mut self, cx: &mut Context<Self>) {
        let name = self
            .visible_rows(cx)
            .get(self.highlighted)
            .map(|row| row.name.clone());
        if let Some(name) = name {
            cx.emit(ProfilePickerEvent::Picked(name));
        }
    }
}

fn profile_row(
    ix: usize,
    row: &ProfileRow,
    highlighted: bool,
    is_current: bool,
    palette: &Palette,
    cx: &Context<ProfilePicker>,
) -> impl IntoElement {
    let name = row.name.clone();
    div()
        .id(SharedString::from(format!("profile-picker.row.{ix}")))
        .debug_selector(move || format!("profile-picker.row.{ix}"))
        .flex()
        .items_center()
        .gap(px(10.0))
        .min_h(px(ROW_HEIGHT))
        .px(px(8.0))
        .rounded(px(6.0))
        .when(highlighted, |item| item.bg(palette.sel))
        .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
            cx.emit(ProfilePickerEvent::Picked(name.clone()));
        }))
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .w(px(BADGE_SIZE))
                .h(px(BADGE_SIZE))
                .rounded(px(BADGE_RADIUS))
                .bg(palette.badge)
                .text_color(white())
                .text_size(px(BADGE_FONT_SIZE))
                .font_weight(FontWeight::BOLD)
                .child(row.badge.clone()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(NAME_FONT_SIZE))
                        .text_color(palette.text)
                        .child(row.name.clone()),
                )
                .child(
                    div()
                        .text_size(px(SUBTITLE_FONT_SIZE))
                        .text_color(palette.dim)
                        .child(row.subtitle.clone()),
                ),
        )
        .when(is_current, |item| {
            item.child(AppIcon::Check.icon().text_color(palette.accent))
        })
}

impl Render for ProfilePicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let palette = Palette::for_dark(dark);
        let visible = self.visible_rows(cx);
        let highlighted = self.highlighted;
        let filter_text = self.filter.read(cx).value();

        let body = if visible.is_empty() {
            div()
                .debug_selector(|| "profile-picker.empty".into())
                .px(px(8.0))
                .py(px(12.0))
                .text_size(px(FONT_SIZE))
                .text_color(palette.dim)
                .child(profile_picker_empty_text(&filter_text))
                .into_any_element()
        } else {
            div()
                .id("profile-picker.list")
                .track_scroll(&self.scroll)
                .overflow_y_scroll()
                .max_h(px(ROW_HEIGHT * VISIBLE_ROWS))
                .flex()
                .flex_col()
                .children(visible.iter().enumerate().map(|(ix, row)| {
                    profile_row(
                        ix,
                        row,
                        ix == highlighted,
                        row.name == self.current,
                        &palette,
                        cx,
                    )
                }))
                .into_any_element()
        };

        let search_row = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(8.0))
            .pb(px(6.0))
            .mb(px(4.0))
            .border_b_1()
            .border_color(palette.line2)
            .child(AppIcon::Search.icon().text_color(palette.faint))
            .child(
                div()
                    .flex_1()
                    .child(Input::new(&self.filter).appearance(false)),
            );

        let connect = div()
            .id("profile-picker.connect")
            .debug_selector(|| "profile-picker.connect".into())
            .flex()
            .items_center()
            .gap(px(8.0))
            .h(px(ROW_HEIGHT))
            .px(px(8.0))
            .mt(px(4.0))
            .border_t_1()
            .border_color(palette.line2)
            .rounded(px(6.0))
            .text_size(px(NAME_FONT_SIZE))
            .text_color(palette.dim)
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                cx.emit(ProfilePickerEvent::ConnectToAws);
            }))
            .child(AppIcon::Plus.icon().text_color(palette.dim))
            .child("Connect to AWS…");

        div()
            .key_context("ProfilePicker")
            .capture_action(cx.listener(|this, _: &MoveUp, _, cx| {
                this.step_highlight(false, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|this, _: &MoveDown, _, cx| {
                this.step_highlight(true, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|this, _: &Enter, _, cx| {
                this.pick_highlighted(cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|_, _: &Escape, _, cx| {
                cx.emit(ProfilePickerEvent::Dismissed);
                cx.stop_propagation();
            }))
            .w(px(PANEL_WIDTH))
            .flex()
            .flex_col()
            .p(px(PANEL_PADDING))
            .bg(palette.surf)
            .border_1()
            .border_color(palette.line)
            .rounded(px(PANEL_RADIUS))
            .shadow_lg()
            .overflow_hidden()
            .child(search_row)
            .child(body)
            .child(connect)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use gpui::{TestAppContext, VisualTestContext};
    use gpui_component::Root;

    type Events = Rc<RefCell<Vec<ProfilePickerEvent>>>;

    fn profiles() -> Vec<ProfileInput> {
        vec![
            ProfileInput {
                name: "prod-readonly".into(),
                account_id: Some("111122223333".into()),
                region: Some("us-east-1".into()),
            },
            ProfileInput {
                name: "staging-admin".into(),
                account_id: Some("111122224444".into()),
                region: Some("us-west-2".into()),
            },
            ProfileInput {
                name: "sandbox".into(),
                account_id: Some("777788889999".into()),
                region: Some("eu-west-1".into()),
            },
        ]
    }

    /// Opens the picker with `query` already in the filter so the first drawn frame
    /// is the filtered one: gpui 0.2.2 never clears `debug_bounds` between frames.
    fn open_picker<'a>(
        cx: &'a mut TestAppContext,
        query: &'static str,
    ) -> (Entity<ProfilePicker>, Events, &'a mut VisualTestContext) {
        let slot = Rc::new(RefCell::new(None));
        let slot_for_window = slot.clone();
        let (_root, cx) = cx.add_window_view(move |window, cx| {
            gpui_component::init(cx);
            let picker =
                cx.new(|cx| ProfilePicker::new(profiles(), "prod-readonly".into(), window, cx));
            picker.update(cx, |picker, cx| {
                picker
                    .filter
                    .update(cx, |input, cx| input.set_value(query, window, cx));
            });
            *slot_for_window.borrow_mut() = Some(picker.clone());
            Root::new(picker, window, cx)
        });
        let picker = slot
            .borrow()
            .as_ref()
            .expect("picker entity should be created")
            .clone();
        let events: Events = Rc::new(RefCell::new(Vec::new()));
        let sink = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&picker, move |_, event: &ProfilePickerEvent, _| {
                sink.borrow_mut().push(event.clone())
            })
            .detach();
        });
        (picker, events, cx)
    }

    fn visible_names(picker: &Entity<ProfilePicker>, cx: &mut VisualTestContext) -> Vec<String> {
        picker.read_with(cx, |picker, cx| {
            picker
                .visible_rows(cx)
                .into_iter()
                .map(|row| row.name)
                .collect()
        })
    }

    #[gpui::test]
    fn renders_a_row_per_profile_and_the_connect_footer(cx: &mut TestAppContext) {
        let (_, _, cx) = open_picker(cx, "");

        assert!(cx.debug_bounds("profile-picker.row.0").is_some());
        assert!(cx.debug_bounds("profile-picker.row.1").is_some());
        assert!(cx.debug_bounds("profile-picker.row.2").is_some());
        assert!(cx.debug_bounds("profile-picker.row.3").is_none());
        assert!(cx.debug_bounds("profile-picker.connect").is_some());
        assert!(cx.debug_bounds("profile-picker.empty").is_none());
    }

    #[gpui::test]
    fn rows_carry_core_badges_with_sandbox_disambiguated(cx: &mut TestAppContext) {
        let (picker, _, cx) = open_picker(cx, "");

        let badges: Vec<String> = picker.read_with(cx, |picker, cx| {
            picker
                .visible_rows(cx)
                .into_iter()
                .map(|row| row.badge)
                .collect()
        });
        assert_eq!(badges, vec!["PR", "SA", "SX"]);
    }

    #[gpui::test]
    fn a_filter_keeps_only_matching_rows(cx: &mut TestAppContext) {
        let (picker, _, cx) = open_picker(cx, "stag");

        assert_eq!(visible_names(&picker, cx), vec!["staging-admin"]);
        assert!(cx.debug_bounds("profile-picker.row.0").is_some());
        assert!(cx.debug_bounds("profile-picker.row.1").is_none());
    }

    #[gpui::test]
    fn a_filter_matching_nothing_renders_the_empty_state(cx: &mut TestAppContext) {
        let (picker, _, cx) = open_picker(cx, "zz");

        assert!(visible_names(&picker, cx).is_empty());
        assert!(cx.debug_bounds("profile-picker.empty").is_some());
        assert!(cx.debug_bounds("profile-picker.row.0").is_none());
    }

    #[gpui::test]
    fn clicking_a_row_picks_it(cx: &mut TestAppContext) {
        let (_, events, cx) = open_picker(cx, "");

        let row = cx
            .debug_bounds("profile-picker.row.1")
            .expect("second row renders");
        cx.simulate_click(row.center(), Default::default());
        cx.run_until_parked();

        assert_eq!(
            *events.borrow(),
            vec![ProfilePickerEvent::Picked("staging-admin".into())]
        );
    }

    #[gpui::test]
    fn clicking_connect_to_aws_emits_connect(cx: &mut TestAppContext) {
        let (_, events, cx) = open_picker(cx, "");

        let connect = cx
            .debug_bounds("profile-picker.connect")
            .expect("connect footer renders");
        cx.simulate_click(connect.center(), Default::default());
        cx.run_until_parked();

        assert_eq!(*events.borrow(), vec![ProfilePickerEvent::ConnectToAws]);
    }

    #[gpui::test]
    fn down_then_enter_picks_the_second_row(cx: &mut TestAppContext) {
        let (_, events, cx) = open_picker(cx, "");

        cx.simulate_keystrokes("down enter");

        assert_eq!(
            *events.borrow(),
            vec![ProfilePickerEvent::Picked("staging-admin".into())]
        );
    }

    #[gpui::test]
    fn down_clamps_at_the_last_row(cx: &mut TestAppContext) {
        let (picker, events, cx) = open_picker(cx, "");

        cx.simulate_keystrokes("down down down down enter");

        assert_eq!(picker.read_with(cx, |picker, _| picker.highlighted), 2);
        assert_eq!(
            *events.borrow(),
            vec![ProfilePickerEvent::Picked("sandbox".into())]
        );
    }

    #[gpui::test]
    fn typing_resets_the_highlight_to_the_first_row(cx: &mut TestAppContext) {
        let (picker, _, cx) = open_picker(cx, "");

        cx.simulate_keystrokes("down");
        assert_eq!(picker.read_with(cx, |picker, _| picker.highlighted), 1);

        cx.simulate_input("s");
        assert_eq!(picker.read_with(cx, |picker, _| picker.highlighted), 0);
    }

    #[gpui::test]
    fn enter_with_no_matching_rows_picks_nothing(cx: &mut TestAppContext) {
        let (_, events, cx) = open_picker(cx, "zz");

        cx.simulate_keystrokes("enter");

        assert!(events.borrow().is_empty());
    }

    #[gpui::test]
    fn escape_dismisses_the_picker(cx: &mut TestAppContext) {
        let (_, events, cx) = open_picker(cx, "");

        cx.simulate_keystrokes("escape");

        assert_eq!(*events.borrow(), vec![ProfilePickerEvent::Dismissed]);
    }
}
