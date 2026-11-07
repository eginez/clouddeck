//! Dense sortable instance table (design/images/01).

use clouddeck_core::{
    CellText, Column, DisplayRow, GroupBy, GroupHeader, Instance, SortState, cell_text,
    display_index_of, is_stale_instance, layout_rules, visible_columns_fit_hidden,
};
use gpui::{
    ClickEvent, Context, Div, Entity, EventEmitter, FocusHandle, Focusable, FontWeight, Hsla,
    IntoElement, KeyBinding, MouseButton, MouseDownEvent, ParentElement, Pixels, Point, Render,
    ScrollStrategy, SharedString, Stateful, Styled, UniformListScrollHandle, Window, actions, div,
    prelude::*, px, uniform_list, white,
};

use crate::{
    fleet_state::FleetState,
    icons::AppIcon,
    tokens::{FONT_SIZE, INSPECTOR_WIDTH, MONO_FONT, Palette, ROW_HEIGHT, SIDEBAR_WIDTH},
    ui_record::{Recorded, forget_prefix, record},
};

const COLUMN_MENU_WIDTH: f32 = 180.0;
const COLUMN_MENU_RADIUS: f32 = 6.0;
const COLUMN_MENU_PADDING: f32 = 4.0;
const COLUMN_MENU_ITEM_RADIUS: f32 = 4.0;

actions!(
    instance_table,
    [
        SelectNext,
        SelectPrevious,
        ToggleColumnMenu,
        ColumnMenuNext,
        ColumnMenuPrevious,
        ColumnMenuToggle,
        ColumnMenuClose,
    ]
);

pub fn table_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("down", SelectNext, Some("InstanceTable")),
        KeyBinding::new("j", SelectNext, Some("InstanceTable")),
        KeyBinding::new("up", SelectPrevious, Some("InstanceTable")),
        KeyBinding::new("k", SelectPrevious, Some("InstanceTable")),
        KeyBinding::new("secondary-alt-c", ToggleColumnMenu, Some("InstanceTable")),
        KeyBinding::new("down", ColumnMenuNext, Some("ColumnMenu")),
        KeyBinding::new("j", ColumnMenuNext, Some("ColumnMenu")),
        KeyBinding::new("up", ColumnMenuPrevious, Some("ColumnMenu")),
        KeyBinding::new("k", ColumnMenuPrevious, Some("ColumnMenu")),
        KeyBinding::new("space", ColumnMenuToggle, Some("ColumnMenu")),
        KeyBinding::new("enter", ColumnMenuToggle, Some("ColumnMenu")),
        KeyBinding::new("escape", ColumnMenuClose, Some("ColumnMenu")),
    ]
}

#[derive(Debug, Clone, PartialEq)]
pub enum InstanceTableEvent {
    RowContextMenu {
        key: String,
        position: Point<Pixels>,
    },
}

pub struct InstanceTable {
    state: Entity<FleetState>,
    focus_handle: FocusHandle,
    column_menu_handle: FocusHandle,
    scroll: UniformListScrollHandle,
}

impl EventEmitter<InstanceTableEvent> for InstanceTable {}

impl InstanceTable {
    pub fn new(state: Entity<FleetState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            focus_handle: cx.focus_handle(),
            column_menu_handle: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
        }
    }

    fn set_column_menu(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.state
            .update(cx, |state, cx| state.set_column_menu_open(open, cx));
        if open {
            self.column_menu_handle.focus(window);
        } else {
            self.focus_handle.focus(window);
        }
    }

    fn toggle_column_menu(
        &mut self,
        _: &ToggleColumnMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let open = !self.state.read(cx).column_menu_open;
        self.set_column_menu(open, window, cx);
    }

    fn column_menu_next(&mut self, _: &ColumnMenuNext, _: &mut Window, cx: &mut Context<Self>) {
        self.state
            .update(cx, |state, cx| state.move_column_menu_focus(true, cx));
    }

    fn column_menu_previous(
        &mut self,
        _: &ColumnMenuPrevious,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state
            .update(cx, |state, cx| state.move_column_menu_focus(false, cx));
    }

    fn column_menu_toggle(&mut self, _: &ColumnMenuToggle, _: &mut Window, cx: &mut Context<Self>) {
        self.state
            .update(cx, |state, cx| state.toggle_focused_column(cx));
    }

    fn column_menu_close(
        &mut self,
        _: &ColumnMenuClose,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_column_menu(false, window, cx);
    }

    fn step_selection(&mut self, down: bool, cx: &mut Context<Self>) {
        self.state
            .update(cx, |state, cx| state.move_selection(down, cx));
        let state = self.state.read(cx);
        let selected = state.selected.as_deref();
        let (rows, display) = state.display_rows();
        if let Some(instance_index) = rows
            .iter()
            .position(|instance| Some(instance.id.as_str()) == selected)
            && let Some(ix) = display_index_of(&display, instance_index)
        {
            self.scroll.scroll_to_item(ix, ScrollStrategy::Center);
        }
    }

    fn select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        self.step_selection(true, cx);
    }

    fn select_previous(&mut self, _: &SelectPrevious, _: &mut Window, cx: &mut Context<Self>) {
        self.step_selection(false, cx);
    }
}

impl Focusable for InstanceTable {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for InstanceTable {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        forget_prefix(cx, "table.");
        let palette = crate::tokens::palette(cx);
        let viewport = window.viewport_size();
        let window_width = f32::from(viewport.width);
        let overlays = layout_rules(window_width, f32::from(viewport.height)).panes_are_overlays;
        let entity = self.state.clone();
        let state = self.state.read(cx);
        let mut pane_width = window_width;
        if !overlays {
            if state.sidebar_open {
                pane_width -= SIDEBAR_WIDTH;
            }
            if state.inspector_open && state.selected_instance().is_some() {
                pane_width -= INSPECTOR_WIDTH;
            }
        }
        let columns = visible_columns_fit_hidden(window_width, pane_width, &state.hidden_columns);
        let sort = state.sort;
        let selected = state.selected.clone();
        let stale = state.view_model.stale_regions().to_vec();
        let (rows, display) = state.display_rows();
        let row_count = display.len();
        let group_by = state.group_by;
        let column_menu_open = state.column_menu_open;
        let column_menu_focus = state.column_menu_focus;
        let hidden = state.hidden_columns.clone();

        for (ix, column) in columns.iter().enumerate() {
            record(
                cx,
                format!("table.header.{ix}"),
                Recorded {
                    text: Some(column.title().to_string()),
                    fg: Some("dim"),
                    ..Default::default()
                },
            );
        }
        if column_menu_open {
            for column in Column::ALL {
                record(
                    cx,
                    format!("table.columns.item.{}", column.key()),
                    Recorded {
                        text: Some(column.title().to_string()),
                        ..Default::default()
                    },
                );
            }
        }

        let columns_button = div()
            .id("table.columns.button")
            .debug_selector(|| "table.columns.button".into())
            .cursor_pointer()
            .text_color(palette.dim)
            .child("Columns")
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                let open = !this.state.read(cx).column_menu_open;
                this.set_column_menu(open, window, cx);
            }));

        let column_menu = if column_menu_open {
            let dismiss = div()
                .debug_selector(|| "table.columns.dismiss".into())
                .occlude()
                .absolute()
                .top(px(ROW_HEIGHT))
                .left_0()
                .right_0()
                .bottom_0()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseDownEvent, window, cx| {
                        this.set_column_menu(false, window, cx);
                    }),
                );
            let menu = div()
                .key_context("ColumnMenu")
                .track_focus(&self.column_menu_handle)
                .debug_selector(|| "table.columns.menu".into())
                .occlude()
                .absolute()
                .top(px(ROW_HEIGHT + COLUMN_MENU_PADDING))
                .right(px(8.))
                .w(px(COLUMN_MENU_WIDTH))
                .p(px(COLUMN_MENU_PADDING))
                .flex()
                .flex_col()
                .bg(palette.surf)
                .border_1()
                .border_color(palette.line)
                .rounded(px(COLUMN_MENU_RADIUS))
                .shadow_lg()
                .children(Column::ALL.into_iter().enumerate().map(|(ix, column)| {
                    column_menu_item(
                        ix,
                        column,
                        !hidden.contains(&column),
                        ix == column_menu_focus,
                        palette,
                        entity.clone(),
                    )
                }));
            vec![dismiss.into_any_element(), menu.into_any_element()]
        } else {
            Vec::new()
        };

        let header = div()
            .debug_selector(|| "table.header".into())
            .flex()
            .items_center()
            .w_full()
            .h(px(ROW_HEIGHT))
            .bg(palette.chrome)
            .border_b_1()
            .border_color(palette.line)
            .children(
                columns
                    .iter()
                    .enumerate()
                    .map(|(ix, column)| header_cell(ix, *column, sort, palette, entity.clone())),
            );

        let toolbar = group_by_toolbar(group_by, palette, entity.clone(), columns_button);
        let row_entity = entity.clone();
        let table = cx.entity();
        let body = uniform_list("table.rows", row_count, move |range, _window, cx| {
            range
                .map(|ix| match &display[ix] {
                    DisplayRow::Header(group) => {
                        group_header(group, palette, row_entity.clone()).into_any_element()
                    }
                    DisplayRow::Instance(index) => {
                        let instance = &rows[*index];
                        let is_selected = selected.as_deref() == Some(instance.id.as_str());
                        let is_stale = is_stale_instance(instance, &stale);
                        let (element, records) = row(
                            instance,
                            &columns,
                            is_selected,
                            is_stale,
                            palette,
                            row_entity.clone(),
                            table.clone(),
                        );
                        for (selector, rec) in records {
                            record(cx, selector, rec);
                        }
                        element.into_any_element()
                    }
                })
                .collect()
        })
        .track_scroll(self.scroll.clone())
        .flex_1()
        .w_full();

        div()
            .key_context("InstanceTable")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::toggle_column_menu))
            .on_action(cx.listener(Self::column_menu_next))
            .on_action(cx.listener(Self::column_menu_previous))
            .on_action(cx.listener(Self::column_menu_toggle))
            .on_action(cx.listener(Self::column_menu_close))
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(palette.surf)
            .text_size(px(FONT_SIZE))
            .text_color(palette.text)
            .child(toolbar)
            .child(header)
            .child(body)
            .children(column_menu)
    }
}

fn column_menu_item(
    ix: usize,
    column: Column,
    shown: bool,
    focused: bool,
    palette: Palette,
    state: Entity<FleetState>,
) -> Stateful<Div> {
    let key = column.key();
    let text_color = if column == Column::Name {
        palette.faint
    } else {
        palette.text
    };
    let mut row = div()
        .id(("table.columns.item", ix))
        .debug_selector(move || format!("table.columns.item.{key}"))
        .on_click(move |_: &ClickEvent, _, cx| {
            state.update(cx, |state, cx| state.toggle_column(column, cx));
        })
        .flex()
        .items_center()
        .gap(px(8.))
        .h(px(ROW_HEIGHT))
        .px(px(8.))
        .rounded(px(COLUMN_MENU_ITEM_RADIUS))
        .text_color(text_color);
    if focused {
        row = row.bg(palette.accent).text_color(white());
    }
    row.child(div().w(px(14.)).flex_shrink_0().when(shown, |check| {
        check.child(AppIcon::Check.icon().text_color(palette.dim))
    }))
    .child(div().flex_1().child(column.title()))
}

fn group_by_toolbar(
    group_by: GroupBy,
    palette: Palette,
    state: Entity<FleetState>,
    columns_button: Stateful<Div>,
) -> Div {
    let button = div()
        .id("table.groupby.button")
        .debug_selector(|| "table.groupby.button".into())
        .cursor_pointer()
        .text_color(palette.dim)
        .child(format!("Group by: {}", group_by.label()))
        .on_click(move |_: &ClickEvent, _, cx| {
            state.update(cx, |state, cx| state.set_group_by(group_by.next(), cx));
        });
    div()
        .debug_selector(|| "table.groupby".into())
        .flex()
        .items_center()
        .justify_between()
        .w_full()
        .h(px(ROW_HEIGHT))
        .px(px(8.))
        .bg(palette.chrome)
        .border_b_1()
        .border_color(palette.line)
        .child(button)
        .child(columns_button)
}

fn group_header(
    header: &GroupHeader,
    palette: Palette,
    state: Entity<FleetState>,
) -> Stateful<Div> {
    let row_key = format!("table.group.{}", header.key);
    let label_key = format!("{row_key}.label");
    let key = header.key.clone();
    let chevron = if header.collapsed {
        AppIcon::ChevronRight
    } else {
        AppIcon::ChevronDown
    };
    let label = div()
        .min_w(px(0.))
        .overflow_hidden()
        .text_ellipsis()
        .whitespace_nowrap()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(palette.text)
        .child(header.label.clone())
        .debug_selector(move || label_key.clone());
    let mut row = div()
        .id(SharedString::from(row_key.clone()))
        .debug_selector(|| row_key.clone())
        .on_click(move |_: &ClickEvent, _, cx| {
            let key = key.clone();
            state.update(cx, |state, cx| state.toggle_group(&key, cx));
        })
        .flex()
        .items_center()
        .gap(px(6.))
        .px(px(8.))
        .w_full()
        .h(px(ROW_HEIGHT))
        .bg(palette.chrome)
        .border_b_1()
        .border_color(palette.line2)
        .child(chevron.icon().text_color(palette.dim))
        .child(label)
        .child(
            div()
                .flex_shrink_0()
                .text_color(palette.faint)
                .child(header.count.to_string()),
        );
    if header.failed {
        row = row.child(
            div()
                .flex_shrink_0()
                .ml(px(8.))
                .text_color(palette.warntext)
                .child("could not load"),
        );
    }
    row
}

fn cell_frame(column: Column) -> Div {
    let frame = div().flex().items_center().px(px(8.)).overflow_hidden();
    if column == Column::Name {
        frame.flex_1().min_w(px(column.default_width()))
    } else {
        frame.w(px(column.default_width())).flex_shrink_0()
    }
}

fn header_cell(
    ix: usize,
    column: Column,
    sort: SortState,
    palette: Palette,
    state: Entity<FleetState>,
) -> Stateful<Div> {
    let mut label = div()
        .flex()
        .items_center()
        .gap(px(4.))
        .min_w(px(0.))
        .overflow_hidden()
        .text_color(palette.dim)
        .font_weight(FontWeight::MEDIUM)
        .child(
            div()
                .min_w(px(0.))
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .child(column.title()),
        );
    if sort.column == column {
        let icon = if sort.ascending {
            AppIcon::ChevronUp
        } else {
            AppIcon::ChevronDown
        };
        label = label.child(icon.icon().text_color(palette.dim));
    }
    cell_frame(column)
        .id(("table.header", ix))
        .debug_selector(move || format!("table.header.{ix}"))
        .on_click(move |_: &ClickEvent, _, cx| {
            state.update(cx, |state, cx| state.toggle_sort(column, cx));
        })
        .child(label)
}

fn row(
    instance: &Instance,
    columns: &[Column],
    selected: bool,
    stale: bool,
    palette: Palette,
    state: Entity<FleetState>,
    table: Entity<InstanceTable>,
) -> (Stateful<Div>, Vec<(String, Recorded)>) {
    let key = instance.id.clone();
    let row_key = format!("table.row.{key}");
    let menu_key = key.clone();
    let mut row = div()
        .id(SharedString::from(row_key.clone()))
        .debug_selector(|| row_key.clone())
        .on_click(move |_: &ClickEvent, _, cx| {
            let key = key.clone();
            state.update(cx, |state, cx| state.select(Some(key), cx));
        })
        .on_mouse_down(MouseButton::Right, move |event: &MouseDownEvent, _, cx| {
            let key = menu_key.clone();
            let position = event.position;
            table.update(cx, |_, cx| {
                cx.emit(InstanceTableEvent::RowContextMenu { key, position })
            });
        })
        .flex()
        .items_center()
        .w_full()
        .h(px(ROW_HEIGHT))
        .border_b_1()
        .border_color(palette.line2);
    if selected {
        row = row.bg(palette.sel);
    }
    let (cells, records): (Vec<Div>, Vec<Option<(String, Recorded)>>) = columns
        .iter()
        .map(|column| cell(instance, *column, &row_key, palette, stale))
        .unzip();
    (row.children(cells), records.into_iter().flatten().collect())
}

/// Text colour token and value for one table cell; the State column mirrors `Palette::state_text`.
fn cell_color(
    palette: &Palette,
    column: Column,
    raw_state: &str,
    stale: bool,
    muted: bool,
) -> (&'static str, Hsla) {
    if stale || muted {
        ("faint", palette.faint)
    } else if column == Column::State {
        match raw_state.to_ascii_lowercase().as_str() {
            "running" => ("runtext", palette.runtext),
            "pending" => ("warntext", palette.warntext),
            _ => ("dim", palette.dim),
        }
    } else {
        ("text", palette.text)
    }
}

fn cell(
    instance: &Instance,
    column: Column,
    row_key: &str,
    palette: Palette,
    stale: bool,
) -> (Div, Option<(String, Recorded)>) {
    let CellText { text, muted } = cell_text(instance, column);
    let raw_state = instance.state.raw.as_str();
    let (fg, color) = cell_color(&palette, column, raw_state, stale, muted);
    let record = match column {
        Column::Name => Some((format!("{row_key}.name"), Some("ui"))),
        Column::InstanceId => Some((format!("{row_key}.id"), Some("mono"))),
        Column::State => Some((format!("{row_key}.state"), None)),
        _ => None,
    }
    .map(|(selector, font)| {
        (
            selector,
            Recorded {
                text: Some(text.clone()),
                fg: Some(fg),
                font,
                ..Default::default()
            },
        )
    });

    let mut label = div()
        .min_w(px(0.))
        .overflow_hidden()
        .text_ellipsis()
        .whitespace_nowrap()
        .text_color(color)
        .child(text);
    if muted {
        label = label.italic();
    }
    if column.monospace() {
        label = label.font_family(MONO_FONT);
    }

    let mut frame = cell_frame(column);
    if column == Column::State {
        frame = frame.child(
            div()
                .w(px(7.))
                .h(px(7.))
                .mr(px(6.))
                .rounded_full()
                .flex_shrink_0()
                .bg(palette.state_dot(raw_state)),
        );
    }
    frame = frame.child(label);
    let frame = match column {
        Column::Name => frame.debug_selector(|| format!("{row_key}.name")),
        Column::InstanceId => frame.debug_selector(|| format!("{row_key}.id")),
        Column::State => frame.debug_selector(|| format!("{row_key}.state")),
        _ => frame,
    };
    (frame, record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::InstanceListViewModel;
    use clouddeck_core::fixture_fleet as fleet;
    use gpui::{
        AppContext, Bounds, TestAppContext, VisualTestContext, WindowBounds, WindowOptions, point,
        size,
    };
    use gpui_component::Root;
    use std::time::Duration;

    use crate::layout_check::states::{State, build};
    use crate::ui_record::recorded;

    fn loaded_state(cx: &mut TestAppContext) -> Entity<FleetState> {
        let mut records = fleet::instances(&[
            ("i-1", "api-server", "running", "t3.medium"),
            ("i-2", "", "stopped", "t3.medium"),
            ("i-3", "worker", "pending", "t3.medium"),
        ]);
        records[1].labels.clear();
        let vm = InstanceListViewModel::new(fleet::static_provider(records));
        let state = cx.new(|_| FleetState::new(vm, "default".to_string(), true));
        state.update(cx, |state, cx| state.refresh(cx));
        wait_until(cx, |cx| {
            state.read_with(cx, |state, _| {
                !state.view_model.instances().is_empty() && !state.view_model.is_loading()
            })
        });
        state
    }

    fn wait_until(cx: &mut TestAppContext, mut condition: impl FnMut(&mut TestAppContext) -> bool) {
        for _ in 0..200 {
            cx.run_until_parked();
            if condition(cx) {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("condition was not satisfied within the timeout");
    }

    /// Opens the table in a window of the given width. Opening at the final width avoids
    /// stale selectors: gpui 0.2.2 keeps `debug_bounds` entries from earlier frames.
    fn open_table(
        cx: &mut TestAppContext,
        state: Entity<FleetState>,
        width: f32,
    ) -> &mut VisualTestContext {
        open_table_view(cx, state, width).1
    }

    fn open_table_view(
        cx: &mut TestAppContext,
        state: Entity<FleetState>,
        width: f32,
    ) -> (Entity<InstanceTable>, &mut VisualTestContext) {
        let table = cx.new(|cx| InstanceTable::new(state, cx));
        let root_table = table.clone();
        let bounds = Bounds::new(point(px(0.), px(0.)), size(px(width), px(760.)));
        let window = cx
            .update(|cx| {
                cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        ..Default::default()
                    },
                    |window, cx| {
                        gpui_component::init(cx);
                        cx.new(|cx| Root::new(root_table, window, cx))
                    },
                )
            })
            .unwrap();
        let cx = VisualTestContext::from_window(window.into(), cx).into_mut();
        cx.run_until_parked();
        (table, cx)
    }

    #[gpui::test]
    fn wide_window_renders_all_eight_column_headers(cx: &mut TestAppContext) {
        let state = loaded_state(cx);
        let cx = open_table(cx, state, 1280.0);

        assert!(cx.debug_bounds("table.header").is_some());
        for key in [
            "table.header.0",
            "table.header.1",
            "table.header.2",
            "table.header.3",
            "table.header.4",
            "table.header.5",
            "table.header.6",
            "table.header.7",
        ] {
            let bounds = cx
                .debug_bounds(key)
                .unwrap_or_else(|| panic!("{key} should render"));
            assert!(bounds.size.width > px(60.), "{key} collapsed to {bounds:?}");
        }
        assert!(cx.debug_bounds("table.header.8").is_none());
    }

    #[gpui::test]
    fn every_row_renders_with_readable_name_and_id_cells(cx: &mut TestAppContext) {
        let state = loaded_state(cx);
        let cx = open_table(cx, state, 1280.0);

        for row in [
            "table.row.us-east-1:i-1",
            "table.row.us-east-1:i-2",
            "table.row.us-east-1:i-3",
        ] {
            assert!(cx.debug_bounds(row).is_some(), "{row} should render");
        }
        for cell in [
            "table.row.us-east-1:i-1.name",
            "table.row.us-east-1:i-1.id",
            "table.row.us-east-1:i-2.name",
            "table.row.us-east-1:i-2.id",
            "table.row.us-east-1:i-3.name",
            "table.row.us-east-1:i-3.id",
        ] {
            let bounds = cx
                .debug_bounds(cell)
                .unwrap_or_else(|| panic!("{cell} should render"));
            assert!(
                bounds.size.width > px(60.),
                "{cell} collapsed to {bounds:?}"
            );
        }
    }

    #[gpui::test]
    fn narrow_window_drops_trailing_columns(cx: &mut TestAppContext) {
        let state = loaded_state(cx);
        let cx = open_table(cx, state, 700.0);

        assert!(cx.debug_bounds("table.header.3").is_some());
        assert!(cx.debug_bounds("table.header.4").is_none());
    }

    #[gpui::test]
    fn sidebar_open_at_960_drops_public_ip_to_fit_the_pane(cx: &mut TestAppContext) {
        let state = loaded_state(cx);
        let cx = open_table(cx, state, 960.0);

        assert!(cx.debug_bounds("table.header.5").is_some());
        assert!(cx.debug_bounds("table.header.6").is_none());
    }

    #[gpui::test]
    fn huge_fleet_renders_only_a_window_of_rows(cx: &mut TestAppContext) {
        let vm = InstanceListViewModel::new(fleet::static_provider(fleet::huge_fleet()));
        let state = cx.new(|_| FleetState::new(vm, "default".to_string(), true));
        state.update(cx, |state, cx| state.refresh(cx));
        wait_until(cx, |cx| {
            state.read_with(cx, |state, _| {
                !state.view_model.instances().is_empty() && !state.view_model.is_loading()
            })
        });
        let keys: Vec<String> = state.read_with(cx, |state, _| {
            state.visible_rows().iter().map(|i| i.id.clone()).collect()
        });
        assert_eq!(keys.len(), 5000);

        let cx = open_table(cx, state, 1280.0);
        let selector = |key: &str| -> &'static str { format!("table.row.{key}").leak() };
        assert!(cx.debug_bounds(selector(&keys[0])).is_some());
        assert!(cx.debug_bounds(selector(&keys[4999])).is_none());
        let rendered = keys[..200]
            .iter()
            .filter(|key| cx.debug_bounds(selector(key)).is_some())
            .count();
        assert!(rendered < 60, "{rendered} of the first 200 rows rendered");
    }

    #[gpui::test]
    fn empty_fleet_renders_no_rows(cx: &mut TestAppContext) {
        let vm = InstanceListViewModel::new(fleet::static_provider(vec![]));
        let state = cx.new(|_| FleetState::new(vm, "default".to_string(), true));
        let cx = open_table(cx, state, 1280.0);

        assert!(cx.debug_bounds("table.header.0").is_some());
        assert!(cx.debug_bounds("table.row.us-east-1:i-1").is_none());
    }

    const ROW_SELECTORS: [&str; 3] = [
        "table.row.us-east-1:i-1",
        "table.row.us-east-1:i-2",
        "table.row.us-east-1:i-3",
    ];

    fn click(cx: &mut VisualTestContext, selector: &'static str) {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} should render"));
        cx.simulate_click(bounds.center(), Default::default());
        cx.run_until_parked();
    }

    fn visible_keys(state: &Entity<FleetState>, cx: &mut VisualTestContext) -> Vec<String> {
        state.read_with(cx, |state, _| {
            state.visible_rows().iter().map(|i| i.id.clone()).collect()
        })
    }

    fn selected_key(state: &Entity<FleetState>, cx: &mut VisualTestContext) -> Option<String> {
        state.read_with(cx, |state, _| state.selected.clone())
    }

    #[gpui::test]
    fn clicking_headers_sorts_and_reverses_direction(cx: &mut TestAppContext) {
        let state = loaded_state(cx);
        let cx = open_table(cx, state.clone(), 1280.0);

        click(cx, "table.header.0");
        state.read_with(cx, |state, _| {
            assert_eq!(
                state.sort,
                SortState {
                    column: Column::Name,
                    ascending: false,
                }
            );
        });

        click(cx, "table.header.0");
        state.read_with(cx, |state, _| {
            assert_eq!(
                state.sort,
                SortState {
                    column: Column::Name,
                    ascending: true,
                }
            );
        });

        click(cx, "table.header.2");
        state.read_with(cx, |state, _| {
            assert_eq!(
                state.sort,
                SortState {
                    column: Column::State,
                    ascending: true,
                }
            );
        });
    }

    #[gpui::test]
    fn clicking_a_row_selects_it_and_opens_the_inspector(cx: &mut TestAppContext) {
        let state = loaded_state(cx);
        let cx = open_table(cx, state.clone(), 1280.0);
        state.update(cx, |state, _| state.inspector_open = false);

        let second = visible_keys(&state, cx)[1].clone();
        let selector = ROW_SELECTORS
            .into_iter()
            .find(|selector| selector.strip_prefix("table.row.") == Some(second.as_str()))
            .expect("second visible row is a fixture row");
        click(cx, selector);

        assert_eq!(selected_key(&state, cx), Some(second));
        state.read_with(cx, |state, _| assert!(state.inspector_open));
    }

    #[gpui::test]
    fn arrow_and_vim_keys_move_the_selection(cx: &mut TestAppContext) {
        let state = loaded_state(cx);
        let (table, cx) = open_table_view(cx, state.clone(), 1280.0);
        cx.update(|_, cx| cx.bind_keys(table_key_bindings()));
        let focus = table.read_with(cx, |table, _| table.focus_handle.clone());
        cx.update(|window, _| focus.focus(window));
        cx.run_until_parked();
        let ids = visible_keys(&state, cx);

        cx.simulate_keystrokes("down");
        assert_eq!(selected_key(&state, cx), Some(ids[0].clone()));

        cx.simulate_keystrokes("j");
        assert_eq!(selected_key(&state, cx), Some(ids[1].clone()));

        cx.simulate_keystrokes("k");
        assert_eq!(selected_key(&state, cx), Some(ids[0].clone()));

        cx.simulate_keystrokes("down");
        assert_eq!(selected_key(&state, cx), Some(ids[1].clone()));

        cx.simulate_keystrokes("up");
        assert_eq!(selected_key(&state, cx), Some(ids[0].clone()));

        cx.simulate_keystrokes("up");
        assert_eq!(selected_key(&state, cx), Some(ids[0].clone()));
    }

    #[gpui::test]
    fn clicking_the_selected_row_keeps_it_selected(cx: &mut TestAppContext) {
        let state = loaded_state(cx);
        let cx = open_table(cx, state.clone(), 1280.0);

        let second = visible_keys(&state, cx)[1].clone();
        let selector = ROW_SELECTORS
            .into_iter()
            .find(|selector| selector.strip_prefix("table.row.") == Some(second.as_str()))
            .expect("second visible row is a fixture row");
        click(cx, selector);
        click(cx, selector);

        assert_eq!(selected_key(&state, cx), Some(second));
    }

    fn recorded_cell(cx: &mut VisualTestContext, key: &str) -> Recorded {
        cx.update(|_, app| recorded(app, key))
            .unwrap_or_else(|| panic!("{key} should be recorded"))
    }

    #[gpui::test]
    fn main_window_records_header_and_first_row_cells(cx: &mut TestAppContext) {
        let (fleet, _main, cx) = build(State::MainIdle, 1280.0, 760.0, cx);
        let fleet = fleet.expect("fleet state has a fleet");
        let id = fleet.read_with(cx, |fleet, _| {
            fleet
                .visible_rows()
                .first()
                .expect("fleet has a visible row")
                .id
                .clone()
        });
        let row = format!("table.row.{id}");

        assert_eq!(
            recorded_cell(cx, "table.header.0").text.as_deref(),
            Some("Name")
        );
        assert_eq!(recorded_cell(cx, &format!("{row}.id")).font, Some("mono"));
        assert!(recorded_cell(cx, &format!("{row}.state")).fg.is_some());
    }

    fn header_titles(cx: &mut VisualTestContext) -> Vec<String> {
        (0..8)
            .filter_map(|ix| {
                cx.update(|_, app| recorded(app, &format!("table.header.{ix}")))
                    .and_then(|recorded| recorded.text)
            })
            .collect()
    }

    #[gpui::test]
    fn column_chooser_hides_and_restores_a_column(cx: &mut TestAppContext) {
        let state = loaded_state(cx);
        let cx = open_table(cx, state.clone(), 1280.0);
        assert!(header_titles(cx).contains(&"Type".to_string()));

        click(cx, "table.columns.button");
        assert!(state.read_with(cx, |state, _| state.column_menu_open));
        assert!(cx.debug_bounds("table.columns.menu").is_some());

        click(cx, "table.columns.item.type");
        assert_eq!(
            state.read_with(cx, |state, _| state.hidden_columns.clone()),
            [Column::Type]
        );
        assert!(!header_titles(cx).contains(&"Type".to_string()));

        click(cx, "table.columns.item.type");
        assert!(header_titles(cx).contains(&"Type".to_string()));
    }

    #[gpui::test]
    fn clicking_the_columns_button_again_closes_the_menu(cx: &mut TestAppContext) {
        let state = loaded_state(cx);
        let cx = open_table(cx, state.clone(), 1280.0);

        click(cx, "table.columns.button");
        click(cx, "table.columns.button");

        assert!(!state.read_with(cx, |state, _| state.column_menu_open));
    }

    #[gpui::test]
    fn clicking_outside_the_chooser_closes_it_without_acting_on_the_table(cx: &mut TestAppContext) {
        let state = loaded_state(cx);
        let cx = open_table(cx, state.clone(), 1280.0);

        click(cx, "table.columns.button");
        click(cx, "table.header.0");

        assert!(!state.read_with(cx, |state, _| state.column_menu_open));
        assert_eq!(
            state.read_with(cx, |state, _| state.sort),
            SortState::default()
        );
    }

    #[gpui::test]
    fn name_column_cannot_be_hidden_from_the_chooser(cx: &mut TestAppContext) {
        let state = loaded_state(cx);
        let cx = open_table(cx, state.clone(), 1280.0);

        click(cx, "table.columns.button");
        click(cx, "table.columns.item.name");

        assert!(state.read_with(cx, |state, _| state.hidden_columns.is_empty()));
        assert!(header_titles(cx).contains(&"Name".to_string()));
    }

    #[gpui::test]
    fn column_chooser_is_operable_from_the_keyboard(cx: &mut TestAppContext) {
        let state = loaded_state(cx);
        let (table, cx) = open_table_view(cx, state.clone(), 1280.0);
        cx.update(|_, cx| cx.bind_keys(table_key_bindings()));
        let focus = table.read_with(cx, |table, _| table.focus_handle.clone());
        cx.update(|window, _| focus.focus(window));
        cx.run_until_parked();

        cx.simulate_keystrokes("secondary-alt-c");
        cx.run_until_parked();
        assert!(state.read_with(cx, |state, _| state.column_menu_open));

        for keys in ["down", "down", "down", "space"] {
            cx.simulate_keystrokes(keys);
            cx.run_until_parked();
        }
        assert_eq!(
            state.read_with(cx, |state, _| state.hidden_columns.clone()),
            [Column::Type]
        );

        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(!state.read_with(cx, |state, _| state.column_menu_open));
        assert!(cx.update(|window, _| focus.is_focused(window)));
    }

    #[test]
    fn state_cells_use_the_palette_state_text_colour() {
        let palette = Palette::light();
        for (state, token) in [
            ("running", "runtext"),
            ("pending", "warntext"),
            ("stopped", "dim"),
        ] {
            assert_eq!(
                cell_color(&palette, Column::State, state, false, false),
                (token, palette.state_text(state))
            );
        }
    }

    #[test]
    fn stale_cells_use_faint_and_plain_cells_use_text() {
        let palette = Palette::light();
        assert_eq!(
            cell_color(&palette, Column::State, "running", true, false),
            ("faint", palette.faint)
        );
        assert_eq!(
            cell_color(&palette, Column::Name, "running", false, false),
            ("text", palette.text)
        );
    }

    #[gpui::test]
    fn narrowing_forgets_the_header_records_of_dropped_columns(cx: &mut TestAppContext) {
        let state = loaded_state(cx);
        let cx = open_table(cx, state, 1280.0);
        assert!(
            cx.update(|_, app| recorded(app, "table.header.7"))
                .is_some()
        );

        cx.simulate_resize(size(px(700.0), px(760.0)));
        cx.run_until_parked();

        assert_eq!(cx.update(|_, app| recorded(app, "table.header.4")), None);
        assert!(
            cx.update(|_, app| recorded(app, "table.header.3"))
                .is_some()
        );
    }
}
