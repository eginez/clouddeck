//! Dense sortable instance table (design/images/01).

use clouddeck_core::{
    CellText, Column, EC2Instance, SortState, cell_text, is_stale_instance, visible_columns,
};
use gpui::{
    Context, Div, Entity, FontWeight, IntoElement, ParentElement, Render, Styled, Window, div,
    prelude::*, px, uniform_list,
};

use crate::{
    fleet_state::FleetState,
    icons::AppIcon,
    tokens::{FONT_SIZE, MONO_FONT, Palette, ROW_HEIGHT},
};

pub struct InstanceTable {
    state: Entity<FleetState>,
}

impl InstanceTable {
    pub fn new(state: Entity<FleetState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self { state }
    }
}

impl Render for InstanceTable {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Palette::light();
        let columns = visible_columns(window.viewport_size().width.into());
        let state = self.state.read(cx);
        let sort = state.sort;
        let selected = state.selected.clone();
        let stale = state.view_model.stale_regions().to_vec();
        let rows = state.visible_rows();
        let row_count = rows.len();

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
                    .map(|(ix, column)| header_cell(ix, *column, sort, palette)),
            );

        let body = uniform_list("table.rows", row_count, move |range, _window, _cx| {
            range
                .map(|ix| {
                    let instance = &rows[ix];
                    let is_selected = selected.as_deref() == Some(instance.id().as_str());
                    let is_stale = is_stale_instance(instance, &stale);
                    row(instance, &columns, is_selected, is_stale, palette)
                })
                .collect()
        })
        .flex_1()
        .w_full();

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.surf)
            .text_size(px(FONT_SIZE))
            .text_color(palette.text)
            .child(header)
            .child(body)
    }
}

fn cell_frame(column: Column) -> Div {
    let frame = div().flex().items_center().px(px(8.)).overflow_hidden();
    if column == Column::Name {
        frame.flex_1().min_w(px(column.default_width()))
    } else {
        frame.w(px(column.default_width())).flex_shrink_0()
    }
}

fn header_cell(ix: usize, column: Column, sort: SortState, palette: Palette) -> Div {
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
        .debug_selector(move || format!("table.header.{ix}"))
        .child(label)
}

fn row(
    instance: &EC2Instance,
    columns: &[Column],
    selected: bool,
    stale: bool,
    palette: Palette,
) -> Div {
    let row_key = format!("table.row.{}", instance.id());
    let mut row = div()
        .debug_selector(|| row_key.clone())
        .flex()
        .items_center()
        .w_full()
        .h(px(ROW_HEIGHT))
        .border_b_1()
        .border_color(palette.line2);
    if selected {
        row = row.bg(palette.sel);
    }
    row.children(
        columns
            .iter()
            .map(|column| cell(instance, *column, &row_key, palette, stale)),
    )
}

fn cell(
    instance: &EC2Instance,
    column: Column,
    row_key: &str,
    palette: Palette,
    stale: bool,
) -> Div {
    let CellText { text, muted } = cell_text(instance, column);
    let raw_state = instance.state.as_deref().unwrap_or_default();
    let color = if stale || muted {
        palette.faint
    } else if column == Column::State {
        palette.state_text(raw_state)
    } else {
        palette.text
    };

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
    match column {
        Column::Name => frame.debug_selector(|| format!("{row_key}.name")),
        Column::InstanceId => frame.debug_selector(|| format!("{row_key}.id")),
        _ => frame,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::InstanceListViewModel;
    use clouddeck_core::{EC2InstanceTag, MockComputeService};
    use gpui::{
        AppContext, Bounds, TestAppContext, VisualTestContext, WindowBounds, WindowOptions, point,
        size,
    };
    use gpui_component::Root;
    use std::time::Duration;

    fn instance(id: &str, name: Option<&str>, state: &str) -> EC2Instance {
        EC2Instance {
            instance_id: id.to_string(),
            region: Some("us-east-1".to_string()),
            instance_type: Some("t3.medium".to_string()),
            state: Some(state.to_string()),
            launch_time: None,
            public_ip_address: None,
            private_ip_address: None,
            vpc_id: None,
            subnet_id: None,
            security_groups: vec![],
            tags: name
                .map(|value| {
                    vec![EC2InstanceTag {
                        key: "Name".to_string(),
                        value: value.to_string(),
                    }]
                })
                .unwrap_or_default(),
            iam_instance_profile_arn: None,
            private_dns_name: None,
            public_dns_name: None,
            availability_zone: None,
            image_id: None,
            key_name: None,
            architecture: None,
            state_transition_reason: None,
        }
    }

    fn fleet() -> Vec<EC2Instance> {
        vec![
            instance("i-1", Some("api-server"), "running"),
            instance("i-2", None, "stopped"),
            instance("i-3", Some("worker"), "pending"),
        ]
    }

    fn loaded_state(cx: &mut TestAppContext) -> Entity<FleetState> {
        let vm = InstanceListViewModel::new(Box::new(MockComputeService::new(fleet(), None)));
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
                        let table = cx.new(|cx| InstanceTable::new(state, cx));
                        cx.new(|cx| Root::new(table, window, cx))
                    },
                )
            })
            .unwrap();
        let cx = VisualTestContext::from_window(window.into(), cx).into_mut();
        cx.run_until_parked();
        cx
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
    fn empty_fleet_renders_no_rows(cx: &mut TestAppContext) {
        let vm = InstanceListViewModel::new(Box::new(MockComputeService::new(vec![], None)));
        let state = cx.new(|_| FleetState::new(vm, "default".to_string(), true));
        let cx = open_table(cx, state, 1280.0);

        assert!(cx.debug_bounds("table.header.0").is_some());
        assert!(cx.debug_bounds("table.row.us-east-1:i-1").is_none());
    }
}
