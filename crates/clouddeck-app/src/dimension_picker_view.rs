//! Dimension multi-select popover (design/images/31).

use clouddeck_core::{ContextModel, DimensionRow, ValueId, move_highlight};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, EventEmitter, KeyDownEvent, Render, SharedString,
    Window, div, prelude::*, px, white,
};
use gpui_component::input::{Enter, Escape, Input, InputEvent, InputState, MoveDown, MoveUp};

use crate::icons::AppIcon;
use crate::tokens::{FONT_SIZE, MONO_FONT, Palette, ROW_HEIGHT};

const PANEL_WIDTH: f32 = 300.0;
const PANEL_RADIUS: f32 = 8.0;
const PANEL_PADDING: f32 = 6.0;
const CHECKBOX_SIZE: f32 = 14.0;
const CHECKBOX_RADIUS: f32 = 4.0;
const INDENT_WIDTH: f32 = 16.0;
const LABEL_FONT_SIZE: f32 = 13.0;
const SMALL_FONT_SIZE: f32 = 11.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DimensionPickerEvent {
    Changed(Vec<ValueId>),
    Dismissed,
}

pub struct DimensionPicker {
    model: ContextModel,
    dimension: String,
    plural: String,
    multi: bool,
    optional: bool,
    filter: Entity<InputState>,
    highlighted: usize,
}

impl EventEmitter<DimensionPickerEvent> for DimensionPicker {}

impl DimensionPicker {
    pub fn new(
        model: ContextModel,
        dimension: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let definition = model
            .dimensions()
            .iter()
            .find(|dim| dim.id.as_str() == dimension);
        let plural = definition.map_or_else(String::new, |dim| dim.plural().to_lowercase());
        let multi = definition.is_some_and(|dim| dim.multi);
        let optional = definition.is_some_and(|dim| dim.optional);
        let filter =
            cx.new(|cx| InputState::new(window, cx).placeholder(format!("Search {plural}")));
        cx.subscribe(&filter, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.highlighted = 0;
                cx.notify();
            }
        })
        .detach();
        filter.update(cx, |input, cx| input.focus(window, cx));
        Self {
            model,
            dimension: dimension.to_string(),
            plural,
            multi,
            optional,
            filter,
            highlighted: 0,
        }
    }

    pub fn rows(&self, cx: &App) -> Vec<DimensionRow> {
        let mut rows = self
            .model
            .rows(&self.dimension, &self.filter.read(cx).value());
        if !self.multi {
            rows.retain(|row| row.value.is_some());
        }
        rows
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

    fn activate(&mut self, row: &DimensionRow, cx: &mut Context<Self>) {
        match &row.value {
            None => self.model.select_all(&self.dimension),
            Some(value) => {
                let selected = self.model.selection().values(&self.dimension).len();
                if !(row.checked && !self.optional && selected <= 1) {
                    self.model.toggle(&self.dimension, value);
                }
            }
        }
        self.emit_changed(cx);
    }

    fn select_only(&mut self, value: &ValueId, cx: &mut Context<Self>) {
        self.model.set_values(&self.dimension, vec![value.clone()]);
        self.emit_changed(cx);
    }

    fn emit_changed(&self, cx: &mut Context<Self>) {
        let values = self.model.selection().values(&self.dimension).to_vec();
        cx.emit(DimensionPickerEvent::Changed(values));
        cx.notify();
    }
}

fn marker(row: &DimensionRow, multi: bool, palette: &Palette) -> AnyElement {
    if !multi {
        return div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .w(px(CHECKBOX_SIZE))
            .h(px(CHECKBOX_SIZE))
            .when(row.checked, |item| {
                item.child(AppIcon::Check.icon().text_color(palette.accent))
            })
            .into_any_element();
    }
    div()
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
        } else if row.implied {
            palette.dim
        } else {
            palette.line
        })
        .when(row.checked, |item| {
            item.bg(palette.accent)
                .child(AppIcon::Check.icon().text_color(white()))
        })
        .into_any_element()
}

fn dimension_row(
    ix: usize,
    row: &DimensionRow,
    highlighted: bool,
    multi: bool,
    palette: &Palette,
    cx: &Context<DimensionPicker>,
) -> impl IntoElement {
    let group = SharedString::from(format!("dimension-picker.row.{ix}"));
    let click_row = row.clone();

    let only_link = row.value.clone().filter(|_| multi).map(|value| {
        div()
            .id(SharedString::from(format!("dimension-picker.only.{ix}")))
            .debug_selector(move || format!("dimension-picker.only.{ix}"))
            .flex_none()
            .px(px(4.0))
            .text_size(px(SMALL_FONT_SIZE))
            .text_color(palette.accent)
            .invisible()
            .group_hover(group.clone(), |style| style.visible())
            .when(highlighted, |item| item.visible())
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.select_only(&value, cx);
            }))
            .child("only")
    });

    let label = div()
        .debug_selector(move || format!("dimension-picker.label.{ix}"))
        .flex_1()
        .min_w_0()
        .text_size(px(LABEL_FONT_SIZE))
        .text_color(palette.text)
        .when(row.value.is_some(), |item| item.font_family(MONO_FONT))
        .child(row.label.clone());

    let count = div()
        .flex_none()
        .text_size(px(LABEL_FONT_SIZE))
        .text_color(palette.dim)
        .child(row.count.to_string());

    div()
        .id(SharedString::from(format!("dimension-picker.row.{ix}")))
        .debug_selector(move || format!("dimension-picker.row.{ix}"))
        .group(group)
        .flex()
        .items_center()
        .gap(px(10.0))
        .min_h(px(ROW_HEIGHT))
        .pl(px(8.0 + INDENT_WIDTH * row.depth as f32))
        .pr(px(8.0))
        .rounded(px(6.0))
        .when(highlighted, |item| item.bg(palette.sel))
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.activate(&click_row, cx);
        }))
        .child(marker(row, multi, palette))
        .child(label)
        .children(only_link)
        .child(count)
}

impl Render for DimensionPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = crate::tokens::palette(cx);
        let rows = self.rows(cx);
        let highlighted = self.highlighted;
        let multi = self.multi;

        let body = if rows.is_empty() {
            let filter = self.filter.read(cx).value();
            div()
                .debug_selector(|| "dimension-picker.empty".into())
                .px(px(8.0))
                .py(px(12.0))
                .text_size(px(FONT_SIZE))
                .text_color(palette.dim)
                .child(format!("No {} match “{}”.", self.plural, filter.trim()))
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .children(rows.iter().enumerate().map(|(ix, row)| {
                    dimension_row(ix, row, ix == highlighted, multi, &palette, cx)
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
            .key_context("DimensionPicker")
            .capture_action(cx.listener(|this, _: &MoveUp, _, cx| {
                this.step_highlight(false, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|this, _: &MoveDown, _, cx| {
                this.step_highlight(true, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|_, _: &Enter, _, cx| {
                cx.emit(DimensionPickerEvent::Dismissed);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|_, _: &Escape, _, cx| {
                cx.emit(DimensionPickerEvent::Dismissed);
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
    use clouddeck_core::{
        CellFailure, DimValue, Dimension, DimensionId, ErrorKind, Instance, ListResult,
        ProviderError, Selection,
    };
    use gpui::{TestAppContext, VisualTestContext};
    use gpui_component::Root;

    type Events = Rc<RefCell<Vec<DimensionPickerEvent>>>;

    fn value(id: &str, parent: Option<&str>) -> DimValue {
        DimValue {
            id: id.into(),
            label: id.to_string(),
            parent: parent.map(ValueId::from),
            default: false,
        }
    }

    fn dimension(id: &str, multi: bool, values: Vec<DimValue>) -> Dimension {
        Dimension {
            id: DimensionId::from(id),
            label: "Region".to_string(),
            plural_label: None,
            multi,
            optional: false,
            values,
        }
    }

    fn flat_region_dimension() -> Dimension {
        dimension(
            "region",
            true,
            ["us-east-1", "us-west-2", "eu-west-1", "eu-central-1"]
                .iter()
                .map(|region| value(region, None))
                .collect(),
        )
    }

    fn model_with(dimensions: Vec<Dimension>) -> ContextModel {
        let mut model = ContextModel::new(Vec::new(), None, "");
        model.set_dimensions(dimensions);
        model
    }

    fn ids(values: &[&str]) -> Vec<ValueId> {
        values.iter().map(|value| ValueId::from(*value)).collect()
    }

    /// Sets the filter before the first draw; gpui 0.2.2 never clears `debug_bounds`.
    fn open_picker<'a>(
        cx: &'a mut TestAppContext,
        model: ContextModel,
        dimension: &'static str,
        query: &'static str,
    ) -> (Entity<DimensionPicker>, Events, &'a mut VisualTestContext) {
        let slot = Rc::new(RefCell::new(None));
        let slot_for_window = slot.clone();
        let (_root, cx) = cx.add_window_view(move |window, cx| {
            gpui_component::init(cx);
            let picker = cx.new(|cx| DimensionPicker::new(model, dimension, window, cx));
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
            cx.subscribe(&picker, move |_, event: &DimensionPickerEvent, _| {
                sink.borrow_mut().push(event.clone())
            })
            .detach();
        });
        (picker, events, cx)
    }

    fn open_region<'a>(
        cx: &'a mut TestAppContext,
        query: &'static str,
    ) -> (Entity<DimensionPicker>, Events, &'a mut VisualTestContext) {
        open_picker(
            cx,
            model_with(vec![flat_region_dimension()]),
            "region",
            query,
        )
    }

    fn selected(picker: &Entity<DimensionPicker>, cx: &mut VisualTestContext) -> Vec<ValueId> {
        picker.read_with(cx, |picker, _| {
            picker.model.selection().values(&picker.dimension).to_vec()
        })
    }

    fn click(selector: &'static str, cx: &mut VisualTestContext) {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} renders"));
        cx.simulate_click(bounds.center(), Default::default());
        cx.run_until_parked();
    }

    #[gpui::test]
    fn renders_all_row_then_one_row_per_region(cx: &mut TestAppContext) {
        let (_, _, cx) = open_region(cx, "");

        assert!(cx.debug_bounds("dimension-picker.row.0").is_some());
        assert!(cx.debug_bounds("dimension-picker.row.1").is_some());
        assert!(cx.debug_bounds("dimension-picker.row.2").is_some());
        assert!(cx.debug_bounds("dimension-picker.row.3").is_some());
        assert!(cx.debug_bounds("dimension-picker.row.4").is_some());
        assert!(cx.debug_bounds("dimension-picker.row.5").is_none());
        assert!(cx.debug_bounds("dimension-picker.empty").is_none());
    }

    #[gpui::test]
    fn clicking_a_region_row_toggles_it(cx: &mut TestAppContext) {
        let (picker, events, cx) = open_region(cx, "");

        click("dimension-picker.row.3", cx);

        let expected = ids(&["us-east-1", "us-west-2", "eu-central-1"]);
        assert_eq!(selected(&picker, cx), expected);
        assert_eq!(
            *events.borrow(),
            vec![DimensionPickerEvent::Changed(expected)]
        );
    }

    #[gpui::test]
    fn only_link_selects_just_that_region(cx: &mut TestAppContext) {
        let (picker, events, cx) = open_region(cx, "");

        cx.simulate_keystrokes("down");
        click("dimension-picker.only.1", cx);

        assert_eq!(selected(&picker, cx), ids(&["us-east-1"]));
        assert_eq!(
            events.borrow().last(),
            Some(&DimensionPickerEvent::Changed(ids(&["us-east-1"])))
        );
    }

    #[gpui::test]
    fn all_row_selects_every_value(cx: &mut TestAppContext) {
        let (picker, _, cx) = open_region(cx, "");
        click("dimension-picker.row.3", cx);
        assert_eq!(selected(&picker, cx).len(), 3);

        click("dimension-picker.row.0", cx);

        assert_eq!(
            selected(&picker, cx),
            ids(&["us-east-1", "us-west-2", "eu-west-1", "eu-central-1"])
        );
    }

    #[gpui::test]
    fn last_selected_region_cannot_be_cleared(cx: &mut TestAppContext) {
        let (picker, events, cx) = open_region(cx, "");
        cx.simulate_keystrokes("down");
        click("dimension-picker.only.1", cx);
        let count = events.borrow().len();

        click("dimension-picker.row.1", cx);

        assert_eq!(selected(&picker, cx), ids(&["us-east-1"]));
        assert_eq!(events.borrow().len(), count + 1);
        assert_eq!(
            events.borrow().last(),
            Some(&DimensionPickerEvent::Changed(ids(&["us-east-1"])))
        );
    }

    #[gpui::test]
    fn space_toggles_the_highlighted_row_when_filter_is_empty(cx: &mut TestAppContext) {
        let (picker, _, cx) = open_region(cx, "");

        cx.simulate_keystrokes("down space");

        assert_eq!(
            selected(&picker, cx),
            ids(&["us-west-2", "eu-west-1", "eu-central-1"])
        );
        assert_eq!(
            picker.read_with(cx, |picker, cx| picker.filter.read(cx).value().to_string()),
            ""
        );
    }

    #[gpui::test]
    fn single_select_replaces_the_value_and_has_no_all_row(cx: &mut TestAppContext) {
        let zone = dimension(
            "zone",
            false,
            vec![value("a", None), value("b", None), value("c", None)],
        );
        let (picker, events, cx) = open_picker(cx, model_with(vec![zone]), "zone", "");
        assert!(cx.debug_bounds("dimension-picker.row.3").is_none());
        assert_eq!(selected(&picker, cx), ids(&["a"]));

        click("dimension-picker.row.2", cx);

        assert_eq!(selected(&picker, cx), ids(&["c"]));
        assert_eq!(
            *events.borrow(),
            vec![DimensionPickerEvent::Changed(ids(&["c"]))]
        );
    }

    #[gpui::test]
    fn tree_rows_nest_children_under_parent(cx: &mut TestAppContext) {
        let tree = dimension(
            "region",
            true,
            vec![
                value("us", None),
                value("us-east-1", Some("us")),
                value("us-west-2", Some("us")),
                value("eu", None),
            ],
        );
        let (picker, _, cx) = open_picker(cx, model_with(vec![tree]), "region", "");

        let rows = picker.read_with(cx, |picker, cx| picker.rows(cx));
        let shape: Vec<(&str, usize)> = rows
            .iter()
            .map(|row| (row.label.as_str(), row.depth))
            .collect();
        assert_eq!(
            shape,
            [
                ("All regions", 0),
                ("us", 0),
                ("us-east-1", 1),
                ("us-west-2", 1),
                ("eu", 0)
            ]
        );
    }

    #[gpui::test]
    fn rows_carry_counts_and_failures_from_the_list_result(cx: &mut TestAppContext) {
        let mut model = model_with(vec![flat_region_dimension()]);
        let instance = Instance {
            id: "i-1".into(),
            region: Some("us-east-1".to_string()),
            ..Instance::default()
        };
        model.set_list_result(
            &ListResult {
                resources: vec![instance.clone(), instance],
                failures: vec![CellFailure {
                    cell: Selection::new().with("region", ids(&["us-west-2"])),
                    error: ProviderError::of(ErrorKind::Timeout),
                }],
            },
            |instance| {
                instance
                    .region
                    .iter()
                    .map(|region| ValueId::from(region.as_str()))
                    .collect()
            },
        );
        let (picker, _, cx) = open_picker(cx, model, "region", "");

        let rows = picker.read_with(cx, |picker, cx| picker.rows(cx));
        assert_eq!(rows[1].label, "us-east-1");
        assert_eq!(rows[1].count, 2);
        assert!(!rows[1].failed);
        assert_eq!(rows[2].label, "us-west-2");
        assert_eq!(rows[2].count, 0);
        assert!(rows[2].failed);
    }

    #[gpui::test]
    fn a_filter_matching_nothing_renders_the_empty_state(cx: &mut TestAppContext) {
        let (picker, _, cx) = open_region(cx, "zz");

        assert!(cx.debug_bounds("dimension-picker.empty").is_some());
        assert!(cx.debug_bounds("dimension-picker.row.0").is_none());
        assert!(picker.read_with(cx, |picker, cx| picker.rows(cx).is_empty()));
    }

    #[gpui::test]
    fn enter_and_escape_dismiss_the_picker(cx: &mut TestAppContext) {
        let (_, events, cx) = open_region(cx, "");

        cx.simulate_keystrokes("enter");
        cx.simulate_keystrokes("escape");

        assert_eq!(
            *events.borrow(),
            vec![
                DimensionPickerEvent::Dismissed,
                DimensionPickerEvent::Dismissed
            ]
        );
    }
}
