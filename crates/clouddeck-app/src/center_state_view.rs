//! Centered empty/loading/error states (design/images/71-79).

use std::time::Duration;

use clouddeck_core::{CenterState, CenterText};
use gpui::{
    Animation, AnimationExt, ClickEvent, Context, EventEmitter, FontWeight, Hsla, Render, Window,
    div, prelude::*, pulsating_between, px,
};
use gpui_component::Sizable;

use crate::icons::AppIcon;
use crate::tokens::{FONT_SIZE, MONO_FONT, Palette};

const CONTENT_MAX_WIDTH: f32 = 360.0;
const LOADING_ICON_SIZE: f32 = 56.0;
const ICON_SIZE: f32 = 28.0;
const TITLE_SIZE: f32 = 15.0;
const PULSE_PERIOD: Duration = Duration::from_millis(1600);
const PULSE_MIN_OPACITY: f32 = 0.4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CenterEvent {
    Action(CenterState),
}

pub struct CenterStateView {
    pub state: CenterState,
    pub text: CenterText,
}

impl EventEmitter<CenterEvent> for CenterStateView {}

impl CenterStateView {
    pub fn new(state: CenterState, text: CenterText) -> Self {
        Self { state, text }
    }
}

fn state_icon(state: &CenterState, palette: &Palette) -> (AppIcon, Hsla) {
    match state {
        CenterState::Loading => (AppIcon::Cloud, palette.accent),
        CenterState::EmptyFleet => (AppIcon::Server, palette.faint),
        CenterState::NoMatch => (AppIcon::Search, palette.faint),
        CenterState::SignedOut => (AppIcon::Key, palette.faint),
        CenterState::Offline => (AppIcon::Plug, palette.faint),
        CenterState::Throttled => (AppIcon::Refresh, palette.warntext),
        CenterState::Error { .. } => (AppIcon::CircleAlert, palette.err),
    }
}

impl Render for CenterStateView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = crate::tokens::palette(cx);
        let (icon, icon_color) = state_icon(&self.state, &palette);

        let icon_element = if self.state == CenterState::Loading {
            icon.icon()
                .with_size(px(LOADING_ICON_SIZE))
                .text_color(icon_color)
                .with_animation(
                    "center.loading-pulse",
                    Animation::new(PULSE_PERIOD)
                        .repeat()
                        .with_easing(pulsating_between(PULSE_MIN_OPACITY, 1.0)),
                    |icon, delta| icon.opacity(delta),
                )
                .into_any_element()
        } else {
            icon.icon()
                .with_size(px(ICON_SIZE))
                .text_color(icon_color)
                .into_any_element()
        };

        let action = self.text.action.clone().map(|label| {
            div()
                .id("center.action")
                .debug_selector(|| "center.action".into())
                .flex()
                .items_center()
                .h(px(28.0))
                .px(px(12.0))
                .mt(px(4.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(palette.line)
                .bg(palette.surf)
                .text_size(px(FONT_SIZE))
                .text_color(palette.text)
                .hover(|button| button.bg(palette.hover))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    cx.emit(CenterEvent::Action(this.state.clone()));
                }))
                .child(label)
        });

        let command = self.text.command.clone().map(|command| {
            div()
                .debug_selector(|| "center.command".into())
                .mt(px(4.0))
                .px(px(10.0))
                .py(px(6.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(palette.line2)
                .bg(palette.panel)
                .font_family(MONO_FONT)
                .text_size(px(FONT_SIZE))
                .text_color(palette.dim)
                .child(command)
        });

        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(8.0))
                    .max_w(px(CONTENT_MAX_WIDTH))
                    .px(px(20.0))
                    .text_center()
                    .child(
                        div()
                            .debug_selector(|| "center.icon".into())
                            .child(icon_element),
                    )
                    .child(
                        div()
                            .debug_selector(|| "center.title".into())
                            .text_size(px(TITLE_SIZE))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(palette.text)
                            .child(self.text.title.clone()),
                    )
                    .child(
                        div()
                            .debug_selector(|| "center.body".into())
                            .text_size(px(FONT_SIZE))
                            .text_color(palette.dim)
                            .child(self.text.body.clone()),
                    )
                    .children(action)
                    .children(command),
            )
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use clouddeck_core::{CenterContext, center_text};
    use gpui::{Entity, TestAppContext, VisualTestContext};
    use gpui_component::Root;

    type Events = Rc<RefCell<Vec<CenterEvent>>>;

    fn context() -> CenterContext {
        CenterContext {
            profile: "prod-readonly".to_string(),
            scope_label: "us-east-1".to_string(),
            scope_is_all: false,
            query: String::new(),
            last_good: Some("12:04".to_string()),
            retry_in_seconds: Some(8),
        }
    }

    fn all_states() -> Vec<CenterState> {
        vec![
            CenterState::Loading,
            CenterState::EmptyFleet,
            CenterState::NoMatch,
            CenterState::SignedOut,
            CenterState::Offline,
            CenterState::Throttled,
            CenterState::Error {
                message: "boom".to_string(),
            },
        ]
    }

    fn open_view(
        cx: &mut TestAppContext,
        state: CenterState,
    ) -> (Entity<CenterStateView>, Events, &mut VisualTestContext) {
        let text = center_text(&state, &context());
        let slot = Rc::new(RefCell::new(None));
        let slot_for_window = slot.clone();
        let (_root, cx) = cx.add_window_view(move |window, cx| {
            gpui_component::init(cx);
            let view = cx.new(|_| CenterStateView::new(state, text));
            *slot_for_window.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view = slot
            .borrow()
            .as_ref()
            .expect("center state view should be created")
            .clone();
        let events: Events = Rc::new(RefCell::new(Vec::new()));
        let sink = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&view, move |_, event: &CenterEvent, _| {
                sink.borrow_mut().push(event.clone())
            })
            .detach();
        });
        (view, events, cx)
    }

    #[gpui::test]
    fn every_state_renders_icon_title_and_body(cx: &mut TestAppContext) {
        for state in all_states() {
            let (_view, _events, cx) = open_view(cx, state.clone());
            assert!(cx.debug_bounds("center.icon").is_some(), "{state:?} icon");
            assert!(cx.debug_bounds("center.title").is_some(), "{state:?} title");
            assert!(cx.debug_bounds("center.body").is_some(), "{state:?} body");
        }
    }

    #[gpui::test]
    fn loading_renders_no_action(cx: &mut TestAppContext) {
        let (_view, _events, cx) = open_view(cx, CenterState::Loading);

        assert!(cx.debug_bounds("center.action").is_none());
    }

    #[gpui::test]
    fn offline_renders_no_action(cx: &mut TestAppContext) {
        let (_view, _events, cx) = open_view(cx, CenterState::Offline);

        assert!(cx.debug_bounds("center.action").is_none());
    }

    #[gpui::test]
    fn signed_out_renders_action_without_command(cx: &mut TestAppContext) {
        let (_view, _events, cx) = open_view(cx, CenterState::SignedOut);

        assert!(cx.debug_bounds("center.command").is_none());
        assert!(cx.debug_bounds("center.action").is_some());
    }

    #[gpui::test]
    fn no_match_action_emits_action_with_state(cx: &mut TestAppContext) {
        let (_view, events, cx) = open_view(cx, CenterState::NoMatch);

        let button = cx
            .debug_bounds("center.action")
            .expect("no-match state should offer an action");
        cx.simulate_click(button.center(), Default::default());
        cx.run_until_parked();

        assert_eq!(
            *events.borrow(),
            vec![CenterEvent::Action(CenterState::NoMatch)]
        );
    }
}
