//! Right inspector pane (design/images/10-14).

use std::time::SystemTime;

use clouddeck_core::{
    EC2Instance, InspectorRow, InspectorSection, NO_TAGS_TEXT, inspector_sections,
    inspector_summary, inspector_tags, inspector_title, security_group_rows, security_groups_title,
    tags_title, tags_toggle_text, visible_tags,
};
use gpui::{
    Context, Entity, FontWeight, IntoElement, Render, Window, WindowAppearance, div, prelude::*, px,
};
use gpui_component::Sizable;

use crate::fleet_state::FleetState;
use crate::icons::AppIcon;
use crate::tokens::{FONT_SIZE, MONO_FONT, Palette, STATUS_FONT_SIZE};

const SIDE_PADDING: f32 = 12.0;
const GAP: f32 = 8.0;
const ROW_GAP: f32 = 6.0;
const SECTION_GAP: f32 = 16.0;
const LABEL_WIDTH: f32 = 150.0;

/// Inspector for the selected instance. Callers must `cx.notify()` after changing `stale_note`.
pub struct InspectorPane {
    state: Entity<FleetState>,
    pub stale_note: Option<String>,
}

impl InspectorPane {
    pub fn new(state: Entity<FleetState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            stale_note: None,
        }
    }
}

impl Render for InspectorPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let palette = Palette::for_dark(dark);

        let state = self.state.read(cx);
        let Some(instance) = state.selected_instance() else {
            return div().into_any_element();
        };
        let tags_show_all = state.tags_show_all;
        let sections = inspector_sections(&instance);

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(palette.panel)
            .text_size(px(FONT_SIZE))
            .text_color(palette.text)
            .child(header(&instance, &palette))
            .child(
                div()
                    .id("inspector-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(SECTION_GAP))
                    .p(px(SIDE_PADDING))
                    .when_some(self.stale_note.as_deref(), |body, note| {
                        body.child(stale_box(note, &palette))
                    })
                    .children(
                        sections
                            .iter()
                            .enumerate()
                            .map(|(index, section)| section_view(index, section, &palette)),
                    )
                    .child(security_groups(&instance, &palette))
                    .child(tags(&instance, tags_show_all, &palette))
                    .child(actions(&palette)),
            )
            .into_any_element()
    }
}

fn header(instance: &EC2Instance, palette: &Palette) -> impl IntoElement {
    let title = match inspector_title(instance) {
        Some(name) => div().child(name),
        None => div().italic().text_color(palette.faint).child("unnamed"),
    }
    .font_weight(FontWeight::SEMIBOLD)
    .debug_selector(|| "inspector.title".into());

    let state_dot = palette.state_dot(instance.state.as_deref().unwrap_or(""));

    div()
        .flex()
        .flex_col()
        .gap(px(ROW_GAP))
        .px(px(SIDE_PADDING))
        .py(px(10.0))
        .bg(palette.hover)
        .border_b_1()
        .border_color(palette.line)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(GAP))
                        .child(div().size(px(8.0)).rounded_full().bg(state_dot))
                        .child(title),
                )
                .child(
                    div()
                        .debug_selector(|| "inspector.close".into())
                        .child(AppIcon::Close.icon().xsmall().text_color(palette.dim)),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .font_family(MONO_FONT)
                        .text_color(palette.dim)
                        .child(instance.instance_id.clone()),
                )
                .child(
                    div()
                        .debug_selector(|| "inspector.copy-id".into())
                        .child(copy_icon(palette)),
                ),
        )
        .child(
            div()
                .debug_selector(|| "inspector.summary".into())
                .text_color(palette.dim)
                .child(inspector_summary(instance, SystemTime::now())),
        )
}

fn stale_box(note: &str, palette: &Palette) -> impl IntoElement {
    div()
        .debug_selector(|| "inspector.stale".into())
        .flex()
        .items_start()
        .gap(px(GAP))
        .p(px(GAP))
        .rounded(px(4.0))
        .bg(palette.hover)
        .text_color(palette.warntext)
        .child(
            AppIcon::TriangleAlert
                .icon()
                .xsmall()
                .text_color(palette.warntext),
        )
        .child(div().child(note.to_string()))
}

fn section_view(index: usize, section: &InspectorSection, palette: &Palette) -> impl IntoElement {
    div()
        .debug_selector(move || format!("inspector.section.{index}"))
        .flex()
        .flex_col()
        .gap(px(ROW_GAP))
        .child(section_title(&section.title, palette))
        .children(
            section
                .rows
                .iter()
                .enumerate()
                .map(|(row_index, row)| row_view(index, row_index, row, palette)),
        )
}

fn row_view(
    section_index: usize,
    row_index: usize,
    row: &InspectorRow,
    palette: &Palette,
) -> impl IntoElement {
    let value = match row.value.as_deref() {
        Some(value) => div()
            .flex()
            .items_center()
            .gap(px(GAP))
            .min_w_0()
            .when(row.monospace, |value| value.font_family(MONO_FONT))
            .child(value.to_string())
            .when(row.copyable, |value| value.child(copy_icon(palette)))
            .into_any_element(),
        None => div()
            .italic()
            .text_color(palette.faint)
            .child(row.empty_text.clone())
            .into_any_element(),
    };

    div()
        .debug_selector(move || format!("inspector.row.{section_index}.{row_index}"))
        .flex()
        .items_start()
        .gap(px(GAP))
        .child(
            div()
                .w(px(LABEL_WIDTH))
                .flex_none()
                .text_color(palette.dim)
                .child(row.label),
        )
        .child(value)
}

fn security_groups(instance: &EC2Instance, palette: &Palette) -> impl IntoElement {
    let groups =
        security_group_rows(instance)
            .into_iter()
            .enumerate()
            .map(|(index, (name, id))| {
                div()
                    .debug_selector(move || format!("inspector.sg.{index}"))
                    .flex()
                    .items_center()
                    .gap(px(GAP))
                    .child(name)
                    .child(
                        div()
                            .font_family(MONO_FONT)
                            .text_color(palette.faint)
                            .child(id),
                    )
                    .child(copy_icon(palette))
            });

    div()
        .flex()
        .flex_col()
        .gap(px(ROW_GAP))
        .child(section_title(&security_groups_title(instance), palette))
        .children(groups)
}

fn tags(instance: &EC2Instance, show_all: bool, palette: &Palette) -> impl IntoElement {
    let all = inspector_tags(instance);
    let shown = visible_tags(&all, show_all);

    let mut section = div()
        .flex()
        .flex_col()
        .gap(px(ROW_GAP))
        .child(section_title(&tags_title(all.len()), palette));

    if all.is_empty() {
        section = section.child(
            div()
                .debug_selector(|| "inspector.no-tags".into())
                .text_color(palette.faint)
                .child(NO_TAGS_TEXT),
        );
    } else {
        section = section.child(
            div().flex().flex_wrap().gap(px(4.0)).children(
                shown
                    .iter()
                    .enumerate()
                    .map(|(index, (key, value))| tag_chip(index, key, value, palette)),
            ),
        );
    }

    if let Some(text) = tags_toggle_text(all.len(), show_all) {
        section = section.child(
            div()
                .debug_selector(|| "inspector.tags-toggle".into())
                .text_color(palette.accent)
                .child(text),
        );
    }
    section
}

fn tag_chip(index: usize, key: &str, value: &str, palette: &Palette) -> impl IntoElement {
    div()
        .debug_selector(move || format!("inspector.tag.{index}"))
        .flex()
        .items_center()
        .gap(px(4.0))
        .px(px(6.0))
        .py(px(1.0))
        .rounded(px(3.0))
        .border_1()
        .border_color(palette.line)
        .bg(palette.tagbg)
        .child(div().text_color(palette.faint).child(key.to_string()))
        .child(div().child(value.to_string()))
}

fn actions(palette: &Palette) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(ROW_GAP))
        .child(section_title("Actions", palette))
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(GAP))
                .child(action_button(
                    "inspector.action.copy-id",
                    AppIcon::Copy,
                    "Copy ID",
                    palette,
                ))
                .child(action_button(
                    "inspector.action.console",
                    AppIcon::ExternalLink,
                    "Open in Console",
                    palette,
                )),
        )
}

fn action_button(
    selector: &'static str,
    icon: AppIcon,
    label: &'static str,
    palette: &Palette,
) -> impl IntoElement {
    div()
        .debug_selector(move || selector.into())
        .flex()
        .items_center()
        .gap(px(6.0))
        .px(px(8.0))
        .h(px(24.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(palette.line)
        .bg(palette.surf)
        .text_color(palette.text)
        .child(icon.icon().xsmall().text_color(palette.dim))
        .child(label)
}

fn section_title(title: &str, palette: &Palette) -> impl IntoElement {
    div()
        .text_size(px(STATUS_FONT_SIZE))
        .text_color(palette.faint)
        .child(title.to_uppercase())
}

fn copy_icon(palette: &Palette) -> impl IntoElement {
    AppIcon::Copy.icon().xsmall().text_color(palette.faint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::InstanceListViewModel;
    use clouddeck_core::{EC2InstanceTag, EC2SecurityGroup, MockComputeService};
    use gpui::{
        Bounds, TestAppContext, VisualTestContext, WindowBounds, WindowOptions, point, size,
    };
    use gpui_component::Root;
    use std::time::Duration;

    const RUNNING_KEY: &str = "us-east-1:i-0123456789abcdef0";
    const UNTAGGED_KEY: &str = "us-east-1:i-0d2fc5662eb309e50";

    fn tag(key: &str, value: &str) -> EC2InstanceTag {
        EC2InstanceTag {
            key: key.to_string(),
            value: value.to_string(),
        }
    }

    fn blank(id: &str) -> EC2Instance {
        EC2Instance {
            instance_id: id.to_string(),
            region: Some("us-east-1".to_string()),
            instance_type: None,
            state: None,
            launch_time: None,
            public_ip_address: None,
            private_ip_address: None,
            vpc_id: None,
            subnet_id: None,
            security_groups: vec![],
            tags: vec![],
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

    fn running_with_ten_tags() -> EC2Instance {
        let mut tags = vec![tag("Name", "api-gateway-01")];
        tags.extend((1..=9).map(|n| tag(&format!("k{n}"), &n.to_string())));
        EC2Instance {
            state: Some("running".to_string()),
            instance_type: Some("t3.medium".to_string()),
            availability_zone: Some("us-east-1a".to_string()),
            private_ip_address: Some("10.0.4.3".to_string()),
            private_dns_name: Some("ip-10-0-4-3.ec2.internal".to_string()),
            vpc_id: Some("vpc-098f0fdb8".to_string()),
            subnet_id: Some("subnet-0d99b9061".to_string()),
            image_id: Some("ami-027d083c0".to_string()),
            key_name: Some("ops-ed25519".to_string()),
            security_groups: vec![
                EC2SecurityGroup {
                    group_id: Some("sg-0d2fc5662".to_string()),
                    group_name: Some("web-sg".to_string()),
                },
                EC2SecurityGroup {
                    group_id: Some("sg-074bb5deb".to_string()),
                    group_name: Some("monitoring-sg".to_string()),
                },
            ],
            tags,
            ..blank("i-0123456789abcdef0")
        }
    }

    fn untagged_stopped() -> EC2Instance {
        EC2Instance {
            state: Some("stopped".to_string()),
            instance_type: Some("m6i.large".to_string()),
            ..blank("i-0d2fc5662eb309e50")
        }
    }

    fn fixture() -> Vec<EC2Instance> {
        vec![running_with_ten_tags(), untagged_stopped()]
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

    fn loaded_state(cx: &mut TestAppContext, instances: Vec<EC2Instance>) -> Entity<FleetState> {
        let vm = InstanceListViewModel::new(Box::new(MockComputeService::new(instances, None)));
        let state = cx.new(|_| FleetState::new(vm, "prod-readonly".to_string(), true));
        state.update(cx, |state, cx| state.refresh(cx));
        wait_until(cx, |cx| {
            state.read_with(cx, |state, _| {
                !state.view_model.instances().is_empty() && !state.view_model.is_loading()
            })
        });
        state
    }

    fn select(state: &Entity<FleetState>, key: &str, cx: &mut TestAppContext) {
        let key = key.to_string();
        state.update(cx, |state, cx| state.select(Some(key), cx));
    }

    /// Opens the pane at its final state: gpui 0.2.2 keeps `debug_bounds` from earlier frames.
    fn open_pane<'a>(
        cx: &'a mut TestAppContext,
        state: Entity<FleetState>,
        stale: Option<&str>,
    ) -> &'a mut VisualTestContext {
        let pane = cx.new(|cx| InspectorPane::new(state, cx));
        if let Some(note) = stale {
            pane.update(cx, |pane, _| pane.stale_note = Some(note.to_string()));
        }
        let root_pane = pane.clone();
        let bounds = Bounds::new(point(px(0.), px(0.)), size(px(340.), px(900.)));
        let window = cx
            .update(|cx| {
                cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        ..Default::default()
                    },
                    |window, cx| {
                        gpui_component::init(cx);
                        cx.new(|cx| Root::new(root_pane, window, cx))
                    },
                )
            })
            .unwrap();
        let cx = VisualTestContext::from_window(window.into(), cx).into_mut();
        cx.run_until_parked();
        cx
    }

    fn assert_rendered(cx: &mut VisualTestContext, keys: &[&'static str]) {
        for key in keys {
            assert!(cx.debug_bounds(key).is_some(), "{key} should render");
        }
    }

    fn assert_absent(cx: &mut VisualTestContext, keys: &[&'static str]) {
        for key in keys {
            assert!(cx.debug_bounds(key).is_none(), "{key} should not render");
        }
    }

    #[gpui::test]
    fn nothing_selected_renders_no_inspector(cx: &mut TestAppContext) {
        let state = loaded_state(cx, fixture());

        let cx = open_pane(cx, state, None);

        assert_absent(cx, &["inspector.title", "inspector.summary"]);
    }

    #[gpui::test]
    fn running_instance_renders_header_sections_groups_and_tags(cx: &mut TestAppContext) {
        let state = loaded_state(cx, fixture());
        select(&state, RUNNING_KEY, cx);

        let cx = open_pane(cx, state, None);

        assert_rendered(
            cx,
            &[
                "inspector.title",
                "inspector.summary",
                "inspector.close",
                "inspector.copy-id",
                "inspector.section.0",
                "inspector.section.1",
                "inspector.row.1.4",
                "inspector.sg.0",
                "inspector.sg.1",
                "inspector.tag.0",
                "inspector.tag.7",
                "inspector.tags-toggle",
                "inspector.action.copy-id",
                "inspector.action.console",
            ],
        );
        assert_absent(
            cx,
            &["inspector.tag.8", "inspector.no-tags", "inspector.stale"],
        );
    }

    #[gpui::test]
    fn show_all_tags_reveals_the_rest(cx: &mut TestAppContext) {
        let state = loaded_state(cx, fixture());
        select(&state, RUNNING_KEY, cx);
        state.update(cx, |state, _| state.tags_show_all = true);

        let cx = open_pane(cx, state, None);

        assert_rendered(cx, &["inspector.tag.9", "inspector.tags-toggle"]);
    }

    #[gpui::test]
    fn untagged_instance_renders_the_no_tags_explanation(cx: &mut TestAppContext) {
        let state = loaded_state(cx, fixture());
        select(&state, UNTAGGED_KEY, cx);

        let cx = open_pane(cx, state, None);

        assert_rendered(cx, &["inspector.title", "inspector.no-tags"]);
        assert_absent(cx, &["inspector.tag.0", "inspector.tags-toggle"]);
    }

    #[gpui::test]
    fn stale_note_renders_when_set(cx: &mut TestAppContext) {
        let state = loaded_state(cx, fixture());
        select(&state, RUNNING_KEY, cx);
        let note = clouddeck_core::stale_note("12:01", "eu-central-1");

        let cx = open_pane(cx, state, Some(&note));

        assert_rendered(cx, &["inspector.stale"]);
    }
}
