//! Region multi-select popover (design/images/31).

use clouddeck_core::{
    RegionPickerRow, RegionScope, activate_region_row, move_highlight, region_picker_empty_text,
    region_picker_rows,
};
use gpui::{
    App, ClickEvent, Context, Entity, EventEmitter, KeyDownEvent, Render, SharedString, Window,
    div, prelude::*, px, white,
};
use gpui_component::input::{Enter, Escape, Input, InputEvent, InputState, MoveDown, MoveUp};

use crate::icons::AppIcon;
use crate::tokens::{FONT_SIZE, MONO_FONT, Palette, ROW_HEIGHT};

const PANEL_WIDTH: f32 = 300.0;
const PANEL_RADIUS: f32 = 8.0;
const PANEL_PADDING: f32 = 6.0;
const CHECKBOX_SIZE: f32 = 14.0;
const CHECKBOX_RADIUS: f32 = 4.0;
const LABEL_FONT_SIZE: f32 = 13.0;
const SMALL_FONT_SIZE: f32 = 11.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegionPickerEvent {
    Changed(Vec<String>),
    Dismissed,
}

pub struct RegionPicker {
    pub scope: RegionScope,
    available: Vec<String>,
    counts: Vec<(String, usize)>,
    filter: Entity<InputState>,
    highlighted: usize,
}

impl EventEmitter<RegionPickerEvent> for RegionPicker {}

impl RegionPicker {
    pub fn new(
        scope: RegionScope,
        available: Vec<String>,
        counts: Vec<(String, usize)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Search regions"));
        cx.subscribe(&filter, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.highlighted = 0;
                cx.notify();
            }
        })
        .detach();
        filter.update(cx, |input, cx| input.focus(window, cx));
        Self {
            scope,
            available,
            counts,
            filter,
            highlighted: 0,
        }
    }

    pub fn rows(&self, cx: &App) -> Vec<RegionPickerRow> {
        region_picker_rows(
            &self.scope,
            &self.available,
            &self.counts,
            &self.filter.read(cx).value(),
        )
    }

    fn step_highlight(&mut self, down: bool, cx: &mut Context<Self>) {
        let len = self.rows(cx).len();
        self.highlighted = move_highlight(self.highlighted, len, down);
        cx.notify();
    }

    fn filter_is_empty(&self, cx: &App) -> bool {
        self.filter.read(cx).value().is_empty()
    }

    fn toggle_highlighted(&mut self, cx: &mut Context<Self>) {
        if !self.filter_is_empty(cx) {
            return;
        }
        let rows = self.rows(cx);
        if let Some(row) = rows.get(self.highlighted) {
            self.activate(row, cx);
        }
    }

    fn activate(&mut self, row: &RegionPickerRow, cx: &mut Context<Self>) {
        activate_region_row(&mut self.scope, row, &self.available);
        self.emit_changed(cx);
    }

    fn select_only(&mut self, region: &str, cx: &mut Context<Self>) {
        self.scope.select_only(region, &self.available);
        self.emit_changed(cx);
    }

    fn emit_changed(&self, cx: &mut Context<Self>) {
        cx.emit(RegionPickerEvent::Changed(self.scope.selected().to_vec()));
        cx.notify();
    }
}

fn region_row(
    ix: usize,
    row: &RegionPickerRow,
    highlighted: bool,
    palette: &Palette,
    cx: &Context<RegionPicker>,
) -> impl IntoElement {
    let group = SharedString::from(format!("region-picker.row.{ix}"));
    let click_row = row.clone();

    let checkbox = div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .w(px(CHECKBOX_SIZE))
        .h(px(CHECKBOX_SIZE))
        .rounded(px(CHECKBOX_RADIUS))
        .border_1()
        .border_color(if row.checked {
            palette.accent
        } else {
            palette.line
        })
        .when(row.checked, |item| {
            item.bg(palette.accent)
                .child(AppIcon::Check.icon().text_color(white()))
        });

    let only_link = row.region.clone().map(|region| {
        div()
            .id(SharedString::from(format!("region-picker.only.{ix}")))
            .debug_selector(move || format!("region-picker.only.{ix}"))
            .flex_none()
            .px(px(4.0))
            .text_size(px(SMALL_FONT_SIZE))
            .text_color(palette.accent)
            .invisible()
            .group_hover(group.clone(), |style| style.visible())
            .when(highlighted, |item| item.visible())
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.select_only(&region, cx);
            }))
            .child("only")
    });

    let label = div()
        .flex_1()
        .min_w_0()
        .text_size(px(LABEL_FONT_SIZE))
        .text_color(palette.text)
        .when(row.region.is_some(), |item| item.font_family(MONO_FONT))
        .child(row.label.clone());

    let count = div()
        .flex_none()
        .text_size(px(LABEL_FONT_SIZE))
        .text_color(palette.dim)
        .child(row.count.to_string());

    div()
        .id(SharedString::from(format!("region-picker.row.{ix}")))
        .debug_selector(move || format!("region-picker.row.{ix}"))
        .group(group)
        .flex()
        .items_center()
        .gap(px(10.0))
        .min_h(px(ROW_HEIGHT))
        .px(px(8.0))
        .rounded(px(6.0))
        .when(highlighted, |item| item.bg(palette.sel))
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.activate(&click_row, cx);
        }))
        .child(checkbox)
        .child(label)
        .children(only_link)
        .child(count)
}

impl Render for RegionPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = crate::tokens::palette(cx);
        let rows = self.rows(cx);
        let highlighted = self.highlighted;

        let body = if rows.is_empty() {
            div()
                .debug_selector(|| "region-picker.empty".into())
                .px(px(8.0))
                .py(px(12.0))
                .text_size(px(FONT_SIZE))
                .text_color(palette.dim)
                .child(region_picker_empty_text(&self.filter.read(cx).value()))
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .children(
                    rows.iter()
                        .enumerate()
                        .map(|(ix, row)| region_row(ix, row, ix == highlighted, &palette, cx)),
                )
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

        let footer = div()
            .px(px(8.0))
            .pt(px(8.0))
            .mt(px(4.0))
            .border_t_1()
            .border_color(palette.line2)
            .text_size(px(SMALL_FONT_SIZE))
            .text_color(palette.faint)
            .child("Space or click toggles · Enter applies and closes");

        div()
            .key_context("RegionPicker")
            .capture_action(cx.listener(|this, _: &MoveUp, _, cx| {
                this.step_highlight(false, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|this, _: &MoveDown, _, cx| {
                this.step_highlight(true, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|_, _: &Enter, _, cx| {
                cx.emit(RegionPickerEvent::Dismissed);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|_, _: &Escape, _, cx| {
                cx.emit(RegionPickerEvent::Dismissed);
                cx.stop_propagation();
            }))
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "space" && this.filter_is_empty(cx) {
                    this.toggle_highlighted(cx);
                    cx.stop_propagation();
                }
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
            .child(footer)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use gpui::{TestAppContext, VisualTestContext};
    use gpui_component::Root;

    type Events = Rc<RefCell<Vec<RegionPickerEvent>>>;

    fn available() -> Vec<String> {
        ["us-east-1", "us-west-2", "eu-west-1", "eu-central-1"]
            .iter()
            .map(|region| region.to_string())
            .collect()
    }

    fn counts() -> Vec<(String, usize)> {
        [
            ("us-east-1", 7),
            ("us-west-2", 5),
            ("eu-west-1", 4),
            ("eu-central-1", 4),
        ]
        .iter()
        .map(|(region, count)| (region.to_string(), *count))
        .collect()
    }

    /// Sets the filter before the first draw; gpui 0.2.2 never clears `debug_bounds`.
    fn open_picker<'a>(
        cx: &'a mut TestAppContext,
        query: &'static str,
    ) -> (Entity<RegionPicker>, Events, &'a mut VisualTestContext) {
        let slot = Rc::new(RefCell::new(None));
        let slot_for_window = slot.clone();
        let (_root, cx) = cx.add_window_view(move |window, cx| {
            gpui_component::init(cx);
            let picker = cx.new(|cx| {
                RegionPicker::new(
                    RegionScope::all(&available()),
                    available(),
                    counts(),
                    window,
                    cx,
                )
            });
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
            cx.subscribe(&picker, move |_, event: &RegionPickerEvent, _| {
                sink.borrow_mut().push(event.clone())
            })
            .detach();
        });
        (picker, events, cx)
    }

    fn selected(picker: &Entity<RegionPicker>, cx: &mut VisualTestContext) -> Vec<String> {
        picker.read_with(cx, |picker, _| picker.scope.selected().to_vec())
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    fn click(selector: &'static str, cx: &mut VisualTestContext) {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} renders"));
        cx.simulate_click(bounds.center(), Default::default());
        cx.run_until_parked();
    }

    #[gpui::test]
    fn renders_all_regions_row_then_one_row_per_region(cx: &mut TestAppContext) {
        let (_, _, cx) = open_picker(cx, "");

        assert!(cx.debug_bounds("region-picker.row.0").is_some());
        assert!(cx.debug_bounds("region-picker.row.1").is_some());
        assert!(cx.debug_bounds("region-picker.row.2").is_some());
        assert!(cx.debug_bounds("region-picker.row.3").is_some());
        assert!(cx.debug_bounds("region-picker.row.4").is_some());
        assert!(cx.debug_bounds("region-picker.row.5").is_none());
        assert!(cx.debug_bounds("region-picker.empty").is_none());
    }

    #[gpui::test]
    fn clicking_a_region_row_toggles_it(cx: &mut TestAppContext) {
        let (picker, events, cx) = open_picker(cx, "");

        click("region-picker.row.2", cx);

        let expected = strings(&["us-east-1", "eu-west-1", "eu-central-1"]);
        assert_eq!(selected(&picker, cx), expected);
        assert_eq!(
            *events.borrow(),
            vec![RegionPickerEvent::Changed(expected)]
        );
    }

    #[gpui::test]
    fn only_link_selects_just_that_region(cx: &mut TestAppContext) {
        let (picker, events, cx) = open_picker(cx, "");

        cx.simulate_keystrokes("down");
        click("region-picker.only.1", cx);

        assert_eq!(selected(&picker, cx), strings(&["us-east-1"]));
        assert_eq!(
            events.borrow().last(),
            Some(&RegionPickerEvent::Changed(strings(&["us-east-1"])))
        );
    }

    #[gpui::test]
    fn all_regions_row_selects_every_region(cx: &mut TestAppContext) {
        let (picker, _, cx) = open_picker(cx, "");
        click("region-picker.row.2", cx);
        assert_eq!(selected(&picker, cx).len(), 3);

        click("region-picker.row.0", cx);

        assert_eq!(selected(&picker, cx), available());
    }

    #[gpui::test]
    fn space_toggles_the_highlighted_region_when_filter_is_empty(cx: &mut TestAppContext) {
        let (picker, _, cx) = open_picker(cx, "");

        cx.simulate_keystrokes("down space");

        assert_eq!(
            selected(&picker, cx),
            strings(&["us-west-2", "eu-west-1", "eu-central-1"])
        );
        assert_eq!(
            picker.read_with(cx, |picker, cx| picker.filter.read(cx).value().to_string()),
            ""
        );
    }

    #[gpui::test]
    fn a_filter_matching_nothing_renders_the_empty_state(cx: &mut TestAppContext) {
        let (picker, _, cx) = open_picker(cx, "zz");

        assert!(cx.debug_bounds("region-picker.empty").is_some());
        assert!(cx.debug_bounds("region-picker.row.0").is_none());
        assert!(picker.read_with(cx, |picker, cx| picker.rows(cx).is_empty()));
    }

    #[gpui::test]
    fn enter_and_escape_dismiss_the_picker(cx: &mut TestAppContext) {
        let (_, events, cx) = open_picker(cx, "");

        cx.simulate_keystrokes("enter");
        cx.simulate_keystrokes("escape");

        assert_eq!(
            *events.borrow(),
            vec![RegionPickerEvent::Dismissed, RegionPickerEvent::Dismissed]
        );
    }
}
