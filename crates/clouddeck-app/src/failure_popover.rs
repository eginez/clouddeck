//! Region failure details popover (design/images/32, 70).

use clouddeck_core::{
    CloudDeckError, copy_errors_text, failure_body, failure_title, region_failure_reason,
    several_failed_title,
};
use gpui::{
    AnyElement, ClickEvent, ClipboardItem, Context, Div, ElementId, EventEmitter, FocusHandle,
    FontWeight, KeyDownEvent, Render, Stateful, Window, WindowAppearance, div, prelude::*, px,
    white,
};

use crate::icons::AppIcon;
use crate::tokens::{FONT_SIZE, MONO_FONT, Palette, ROW_HEIGHT};

const POPOVER_WIDTH: f32 = 380.0;
const POPOVER_PADDING: f32 = 14.0;
const POPOVER_RADIUS: f32 = 10.0;
const BUTTON_HEIGHT: f32 = 28.0;
const BUTTON_RADIUS: f32 = 6.0;
const MESSAGE_RADIUS: f32 = 6.0;
const REGION_WIDTH: f32 = 120.0;
const TITLE_SIZE: f32 = 13.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionFailure {
    pub region: String,
    pub message: String,
    pub error: CloudDeckError,
    pub stale_instances: usize,
    pub retry_in_seconds: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailurePopoverEvent {
    Retry(String),
    RetryAll,
    Copied,
    Dismissed,
}

pub struct FailurePopover {
    pub failures: Vec<RegionFailure>,
    pub refreshed_at: String,
    pub other_regions: usize,
    focus: Option<FocusHandle>,
}

impl EventEmitter<FailurePopoverEvent> for FailurePopover {}

impl FailurePopover {
    pub fn new(failures: Vec<RegionFailure>, refreshed_at: String, other_regions: usize) -> Self {
        Self {
            failures,
            refreshed_at,
            other_regions,
            focus: None,
        }
    }

    fn copy_errors(&self, cx: &mut Context<Self>) {
        let pairs: Vec<(String, String)> = self
            .failures
            .iter()
            .map(|failure| (failure.region.clone(), failure.message.clone()))
            .collect();
        cx.write_to_clipboard(ClipboardItem::new_string(copy_errors_text(&pairs)));
        cx.emit(FailurePopoverEvent::Copied);
    }
}

fn button(
    key: String,
    label: &'static str,
    icon: AppIcon,
    primary: bool,
    palette: &Palette,
) -> Stateful<Div> {
    let (background, text, border) = if primary {
        (palette.accent, white(), palette.accent)
    } else {
        (palette.surf, palette.text, palette.line)
    };
    div()
        .id(ElementId::Name(key.clone().into()))
        .debug_selector(move || key)
        .flex()
        .items_center()
        .gap(px(6.0))
        .h(px(BUTTON_HEIGHT))
        .px(px(12.0))
        .rounded(px(BUTTON_RADIUS))
        .border_1()
        .border_color(border)
        .bg(background)
        .text_size(px(FONT_SIZE))
        .text_color(text)
        .child(icon.icon().text_color(text))
        .child(label)
}

impl Render for FailurePopover {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let palette = Palette::for_dark(dark);
        let focus = match &self.focus {
            Some(focus) => focus.clone(),
            None => {
                let focus = cx.focus_handle();
                focus.focus(window);
                self.focus = Some(focus.clone());
                focus
            }
        };

        let content: AnyElement = match self.failures.as_slice() {
            [] => return div().into_any_element(),
            [only] => self.single_failure(only, &palette, cx).into_any_element(),
            many => self.several_failures(many, &palette, cx).into_any_element(),
        };

        div()
            .key_context("FailurePopover")
            .track_focus(&focus)
            .on_key_down(cx.listener(|_, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    cx.emit(FailurePopoverEvent::Dismissed);
                    cx.stop_propagation();
                }
            }))
            .w(px(POPOVER_WIDTH))
            .p(px(POPOVER_PADDING))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .bg(palette.surf)
            .border_1()
            .border_color(palette.line)
            .rounded(px(POPOVER_RADIUS))
            .shadow_lg()
            .child(content)
            .into_any_element()
    }
}

impl FailurePopover {
    fn single_failure(
        &self,
        failure: &RegionFailure,
        palette: &Palette,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let region = failure.region.clone();
        let copy = button(
            "failure.copy".into(),
            "Copy error",
            AppIcon::Copy,
            false,
            palette,
        )
        .hover(|button| button.bg(palette.hover))
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.copy_errors(cx)));
        let retry = button(
            "failure.retry".into(),
            "Retry region",
            AppIcon::Refresh,
            true,
            palette,
        )
        .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
            cx.emit(FailurePopoverEvent::Retry(region.clone()));
        }));

        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(AppIcon::TriangleAlert.icon().text_color(palette.warntext))
                    .child(
                        div()
                            .debug_selector(|| "failure.title".into())
                            .text_size(px(TITLE_SIZE))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(palette.warntext)
                            .child(failure_title(&failure.region)),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "failure.body".into())
                    .text_size(px(FONT_SIZE))
                    .text_color(palette.dim)
                    .child(failure_body(
                        failure.stale_instances,
                        &self.refreshed_at,
                        self.other_regions,
                    )),
            )
            .child(
                div()
                    .debug_selector(|| "failure.message".into())
                    .p(px(8.0))
                    .rounded(px(MESSAGE_RADIUS))
                    .border_1()
                    .border_color(palette.line2)
                    .bg(palette.panel)
                    .font_family(MONO_FONT)
                    .text_size(px(FONT_SIZE))
                    .text_color(palette.text)
                    .child(failure.message.clone()),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(copy)
                    .child(retry),
            )
    }

    fn several_failures(
        &self,
        failures: &[RegionFailure],
        palette: &Palette,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let rows: Vec<AnyElement> = failures
            .iter()
            .enumerate()
            .map(|(ix, failure)| {
                let region = failure.region.clone();
                let retry = button(
                    format!("failure.row.{ix}.retry"),
                    "Retry",
                    AppIcon::Refresh,
                    false,
                    palette,
                )
                .hover(|button| button.bg(palette.hover))
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                    cx.emit(FailurePopoverEvent::Retry(region.clone()));
                }));

                div()
                    .debug_selector(move || format!("failure.row.{ix}"))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .h(px(ROW_HEIGHT))
                    .child(
                        div()
                            .w(px(REGION_WIDTH))
                            .font_family(MONO_FONT)
                            .text_size(px(FONT_SIZE))
                            .text_color(palette.text)
                            .child(failure.region.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(FONT_SIZE))
                            .text_color(palette.dim)
                            .child(region_failure_reason(
                                &failure.error,
                                failure.retry_in_seconds,
                            )),
                    )
                    .child(retry)
                    .into_any_element()
            })
            .collect();

        let copy = button(
            "failure.copy".into(),
            "Copy errors",
            AppIcon::Copy,
            false,
            palette,
        )
        .hover(|button| button.bg(palette.hover))
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.copy_errors(cx)));
        let retry_all = button(
            "failure.retry-all".into(),
            "Retry all",
            AppIcon::Refresh,
            true,
            palette,
        )
        .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
            cx.emit(FailurePopoverEvent::RetryAll);
        }));

        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(AppIcon::TriangleAlert.icon().text_color(palette.warntext))
                    .child(
                        div()
                            .debug_selector(|| "failure.title".into())
                            .text_size(px(TITLE_SIZE))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(palette.warntext)
                            .child(several_failed_title(failures.len())),
                    ),
            )
            .child(div().flex().flex_col().children(rows))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(copy)
                    .child(retry_all),
            )
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use gpui::{Entity, TestAppContext, VisualTestContext};
    use gpui_component::Root;

    type Events = Rc<RefCell<Vec<FailurePopoverEvent>>>;

    fn failure(region: &str, message: &str) -> RegionFailure {
        RegionFailure {
            region: region.to_string(),
            message: message.to_string(),
            error: CloudDeckError::Permission,
            stale_instances: 4,
            retry_in_seconds: None,
        }
    }

    fn open_popover(
        cx: &mut TestAppContext,
        failures: Vec<RegionFailure>,
    ) -> (Entity<FailurePopover>, Events, &mut VisualTestContext) {
        let slot = Rc::new(RefCell::new(None));
        let slot_for_window = slot.clone();
        let (_root, cx) = cx.add_window_view(move |window, cx| {
            gpui_component::init(cx);
            let popover = cx.new(|_| FailurePopover::new(failures, "12:01".to_string(), 3));
            *slot_for_window.borrow_mut() = Some(popover.clone());
            Root::new(popover, window, cx)
        });
        let popover = slot
            .borrow()
            .as_ref()
            .expect("popover entity should be created")
            .clone();
        let events: Events = Rc::new(RefCell::new(Vec::new()));
        let sink = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&popover, move |_, event: &FailurePopoverEvent, _| {
                sink.borrow_mut().push(event.clone())
            })
            .detach();
        });
        (popover, events, cx)
    }

    #[gpui::test]
    fn one_failure_renders_details_and_actions(cx: &mut TestAppContext) {
        let (_, _, cx) = open_popover(
            cx,
            vec![failure("eu-central-1", "AccessDenied: not allowed")],
        );

        assert!(cx.debug_bounds("failure.title").is_some());
        assert!(cx.debug_bounds("failure.body").is_some());
        assert!(cx.debug_bounds("failure.message").is_some());
        assert!(cx.debug_bounds("failure.copy").is_some());
        assert!(cx.debug_bounds("failure.retry").is_some());
        assert!(cx.debug_bounds("failure.row.0").is_none());
    }

    #[gpui::test]
    fn retry_region_emits_retry_for_the_region(cx: &mut TestAppContext) {
        let (_, events, cx) = open_popover(
            cx,
            vec![failure("eu-central-1", "AccessDenied: not allowed")],
        );

        let retry = cx
            .debug_bounds("failure.retry")
            .expect("retry region button renders");
        cx.simulate_click(retry.center(), Default::default());
        cx.run_until_parked();

        assert_eq!(
            *events.borrow(),
            vec![FailurePopoverEvent::Retry("eu-central-1".to_string())]
        );
    }

    #[gpui::test]
    fn copy_error_writes_region_and_message_to_clipboard(cx: &mut TestAppContext) {
        let (_, events, cx) = open_popover(
            cx,
            vec![failure("eu-central-1", "AccessDenied: not allowed")],
        );

        let copy = cx
            .debug_bounds("failure.copy")
            .expect("copy error button renders");
        cx.simulate_click(copy.center(), Default::default());
        cx.run_until_parked();

        assert_eq!(
            cx.read_from_clipboard().and_then(|item| item.text()),
            Some("eu-central-1: AccessDenied: not allowed".to_string())
        );
        assert_eq!(*events.borrow(), vec![FailurePopoverEvent::Copied]);
    }

    #[gpui::test]
    fn several_failures_render_a_row_per_region(cx: &mut TestAppContext) {
        let (_, _, cx) = open_popover(
            cx,
            vec![
                failure("us-west-2", "Throttled"),
                failure("eu-west-1", "AccessDenied"),
                failure("eu-central-1", "AccessDenied"),
            ],
        );

        assert!(cx.debug_bounds("failure.row.2").is_some());
        assert!(cx.debug_bounds("failure.row.2.retry").is_some());
        assert!(cx.debug_bounds("failure.retry-all").is_some());
        assert!(cx.debug_bounds("failure.message").is_none());
    }

    #[gpui::test]
    fn retry_all_emits_retry_all(cx: &mut TestAppContext) {
        let (_, events, cx) = open_popover(
            cx,
            vec![
                failure("us-west-2", "Throttled"),
                failure("eu-west-1", "AccessDenied"),
                failure("eu-central-1", "AccessDenied"),
            ],
        );

        let retry_all = cx
            .debug_bounds("failure.retry-all")
            .expect("retry all button renders");
        cx.simulate_click(retry_all.center(), Default::default());
        cx.run_until_parked();

        assert_eq!(*events.borrow(), vec![FailurePopoverEvent::RetryAll]);
    }

    #[gpui::test]
    fn row_retry_emits_retry_for_that_row_region(cx: &mut TestAppContext) {
        let (_, events, cx) = open_popover(
            cx,
            vec![
                failure("us-west-2", "Throttled"),
                failure("eu-west-1", "AccessDenied"),
                failure("eu-central-1", "AccessDenied"),
            ],
        );

        let retry = cx
            .debug_bounds("failure.row.1.retry")
            .expect("second row retry renders");
        cx.simulate_click(retry.center(), Default::default());
        cx.run_until_parked();

        assert_eq!(
            *events.borrow(),
            vec![FailurePopoverEvent::Retry("eu-west-1".to_string())]
        );
    }

    #[gpui::test]
    fn copy_errors_writes_every_region_to_clipboard(cx: &mut TestAppContext) {
        let (_, events, cx) = open_popover(
            cx,
            vec![
                failure("us-west-2", "Throttled"),
                failure("eu-west-1", "AccessDenied"),
            ],
        );

        let copy = cx
            .debug_bounds("failure.copy")
            .expect("copy errors button renders");
        cx.simulate_click(copy.center(), Default::default());
        cx.run_until_parked();

        assert_eq!(
            cx.read_from_clipboard().and_then(|item| item.text()),
            Some("us-west-2: Throttled\neu-west-1: AccessDenied".to_string())
        );
        assert_eq!(*events.borrow(), vec![FailurePopoverEvent::Copied]);
    }

    #[gpui::test]
    fn escape_dismisses_the_popover(cx: &mut TestAppContext) {
        let (_, events, cx) = open_popover(cx, vec![failure("eu-central-1", "AccessDenied")]);

        cx.simulate_keystrokes("escape");

        assert_eq!(*events.borrow(), vec![FailurePopoverEvent::Dismissed]);
    }
}
