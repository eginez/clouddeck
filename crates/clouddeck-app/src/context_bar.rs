//! Bottom status bar: provider, profile, region, counts, status (design/images/01).

use clouddeck_core::{
    StatusItem, instance_count_text, layout_rules, shown_count_text, status_item,
};
use gpui::{
    AnyElement, AnyView, AppContext, ClickEvent, Context, Entity, EventEmitter, IntoElement,
    Render, Window, div, prelude::*, px,
};
use gpui_component::{Sizable, tooltip::Tooltip};

use crate::fleet_state::FleetState;
use crate::icons::AppIcon;
use crate::tokens::{Palette, STATUS_BAR_HEIGHT, STATUS_FONT_SIZE};
use crate::ui_record::{Recorded, forget_prefix, record};

const SIDE_PADDING: f32 = 12.0;
const GAP: f32 = 10.0;
const CONTROL_HEIGHT: f32 = 20.0;
const CONTROL_RADIUS: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextBarEvent {
    OpenProfilePicker,
    OpenRegionPicker,
    OpenFailureDetails,
    Refresh,
    ToggleInspector,
    ToggleSidebar,
}

/// Callers that change the public fields must `cx.notify()` the bar themselves.
pub struct ContextBar {
    state: Entity<FleetState>,
    pub region_label: String,
    pub failed_regions: Vec<String>,
    pub last_success_text: String,
    pub load_error_text: Option<String>,
}

impl EventEmitter<ContextBarEvent> for ContextBar {}

impl ContextBar {
    pub fn new(state: Entity<FleetState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            region_label: "All regions".to_string(),
            failed_regions: Vec::new(),
            last_success_text: String::new(),
            load_error_text: None,
        }
    }
}

impl Render for ContextBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        forget_prefix(cx, "bar.");
        let palette = crate::tokens::palette(cx);
        let viewport = window.viewport_size();
        let rules = layout_rules(viewport.width.into(), viewport.height.into());

        let state = self.state.read(cx);
        let total = state.view_model.instances().len();
        let shown = state.visible_rows().len();
        let item = status_item(
            state.flash.as_deref(),
            state.view_model.is_loading(),
            &state.profile,
            &self.failed_regions,
        );
        let profile = state.profile.clone();
        let demo = state.demo;
        let inspector_open = state.inspector_open;
        let sidebar_open = state.sidebar_open;

        record(cx, "bar.profile", text_in(profile.clone(), "text"));
        record(cx, "bar.region", text_in(self.region_label.clone(), "text"));
        if item.is_warning() {
            record(cx, "bar.failure", text_in(item.text(), "warntext"));
        } else {
            record(cx, "bar.status", text_in(item.text(), "dim"));
        }

        let mut left = div()
            .flex()
            .items_center()
            .gap(px(GAP))
            .child(aws_mark(demo, &palette))
            .child(picker_button(
                "bar.profile",
                AppIcon::User,
                profile,
                "Switch profile  ⌘P",
                ContextBarEvent::OpenProfilePicker,
                &palette,
                cx,
            ))
            .child(picker_button(
                "bar.region",
                AppIcon::Globe,
                self.region_label.clone(),
                "Switch region  ⌘⇧R",
                ContextBarEvent::OpenRegionPicker,
                &palette,
                cx,
            ));
        if rules.show_instance_count {
            let count = instance_count_text(total);
            record(cx, "bar.count", text_in(count.clone(), "dim"));
            let mut counts = div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(div().debug_selector(|| "bar.count".into()).child(count));
            if rules.show_shown_count {
                let shown_text = format!("· {}", shown_count_text(shown));
                record(cx, "bar.shown", text_in(shown_text.clone(), "dim"));
                counts = counts.child(
                    div()
                        .debug_selector(|| "bar.shown".into())
                        .child(shown_text),
                );
            }
            left = left.child(counts);
        }

        let mut right = div().flex().items_center().gap(px(GAP)).min_w_0();
        if let Some(error_text) = &self.load_error_text {
            right = right.child(
                div()
                    .debug_selector(|| "bar.load-error".into())
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_color(palette.err)
                    .child(error_text.clone()),
            );
        }
        if rules.show_updated_time && !self.last_success_text.is_empty() {
            record(
                cx,
                "bar.updated",
                text_in(self.last_success_text.clone(), "dim"),
            );
            right = right.child(
                div()
                    .debug_selector(|| "bar.updated".into())
                    .child(self.last_success_text.clone()),
            );
        }
        if rules.show_status_bar_buttons {
            right = right
                .child(icon_button(
                    "bar.refresh",
                    AppIcon::Refresh,
                    false,
                    ContextBarEvent::Refresh,
                    &palette,
                    cx,
                ))
                .child(icon_button(
                    "bar.toggle-inspector",
                    AppIcon::PanelRight,
                    inspector_open,
                    ContextBarEvent::ToggleInspector,
                    &palette,
                    cx,
                ))
                .child(icon_button(
                    "bar.toggle-sidebar",
                    AppIcon::PanelLeft,
                    sidebar_open,
                    ContextBarEvent::ToggleSidebar,
                    &palette,
                    cx,
                ));
        }
        right = right.child(status_element(&item, &palette, cx));

        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .h(px(STATUS_BAR_HEIGHT))
            .px(px(SIDE_PADDING))
            .gap(px(GAP))
            .bg(palette.chrome)
            .border_t_1()
            .border_color(palette.line)
            .text_size(px(STATUS_FONT_SIZE))
            .text_color(palette.dim)
            .child(left)
            .child(right)
    }
}

fn text_in(text: String, fg: &'static str) -> Recorded {
    Recorded {
        text: Some(text),
        fg: Some(fg),
        ..Default::default()
    }
}

fn aws_mark(demo: bool, palette: &Palette) -> impl IntoElement {
    div()
        .debug_selector(|| "bar.aws".into())
        .flex()
        .items_center()
        .gap(px(6.0))
        .child(AppIcon::Cloud.icon().xsmall().text_color(palette.dim))
        .child("AWS")
        .when(demo, |row| {
            row.child(
                div()
                    .px(px(4.0))
                    .py(px(1.0))
                    .rounded(px(3.0))
                    .bg(palette.tagbg)
                    .text_color(palette.faint)
                    .child("DEMO"),
            )
        })
}

fn picker_button(
    id: &'static str,
    icon: AppIcon,
    label: String,
    tooltip: &'static str,
    event: ContextBarEvent,
    palette: &Palette,
    cx: &Context<ContextBar>,
) -> impl IntoElement {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .flex()
        .items_center()
        .gap(px(4.0))
        .px(px(6.0))
        .h(px(CONTROL_HEIGHT))
        .rounded(px(CONTROL_RADIUS))
        .text_color(palette.text)
        .hover(|button| button.bg(palette.hover))
        .tooltip(move |_, cx| AnyView::from(cx.new(|_| Tooltip::new(tooltip))))
        .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(event)))
        .child(icon.icon().xsmall().text_color(palette.dim))
        .child(label)
        .child(
            AppIcon::ChevronDown
                .icon()
                .xsmall()
                .text_color(palette.faint),
        )
}

fn icon_button(
    selector: &'static str,
    icon: AppIcon,
    active: bool,
    event: ContextBarEvent,
    palette: &Palette,
    cx: &Context<ContextBar>,
) -> impl IntoElement {
    div()
        .id(selector)
        .debug_selector(move || selector.into())
        .flex()
        .items_center()
        .justify_center()
        .w(px(24.0))
        .h(px(CONTROL_HEIGHT))
        .rounded(px(CONTROL_RADIUS))
        .when(active, |button| button.bg(palette.hover))
        .hover(|button| button.bg(palette.hover))
        .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(event)))
        .child(icon.icon().xsmall().text_color(palette.dim))
}

fn status_element(item: &StatusItem, palette: &Palette, cx: &Context<ContextBar>) -> AnyElement {
    if item.is_warning() {
        div()
            .id("bar.failure")
            .debug_selector(|| "bar.failure".into())
            .flex()
            .items_center()
            .gap(px(4.0))
            .px(px(6.0))
            .h(px(CONTROL_HEIGHT))
            .rounded(px(CONTROL_RADIUS))
            .text_color(palette.warntext)
            .hover(|button| button.bg(palette.hover))
            .on_click(
                cx.listener(|_, _: &ClickEvent, _, cx| {
                    cx.emit(ContextBarEvent::OpenFailureDetails)
                }),
            )
            .child(
                AppIcon::TriangleAlert
                    .icon()
                    .xsmall()
                    .text_color(palette.warntext),
            )
            .child(item.text())
            .into_any_element()
    } else {
        div()
            .debug_selector(|| "bar.status".into())
            .child(item.text())
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::InstanceListViewModel;
    use clouddeck_core::{EC2Instance, EC2InstanceTag, MockComputeService};
    use gpui::{
        Bounds, TestAppContext, VisualTestContext, WindowBounds, WindowOptions, point, size,
    };
    use gpui_component::Root;
    use std::{cell::RefCell, rc::Rc, time::Duration};

    use crate::layout_check::states::{State, build};
    use crate::ui_record::recorded;

    type Events = Rc<RefCell<Vec<ContextBarEvent>>>;

    fn instance(id: &str, name: &str, state: &str) -> EC2Instance {
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
            tags: vec![EC2InstanceTag {
                key: "Name".to_string(),
                value: name.to_string(),
            }],
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

    fn loaded_state(cx: &mut TestAppContext) -> Entity<FleetState> {
        let instances = vec![
            instance("i-1", "api-server", "running"),
            instance("i-2", "bastion", "stopped"),
            instance("i-3", "worker", "stopped"),
        ];
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

    /// Opens the bar in a window of the given size. Opening at the final size avoids
    /// stale selectors: gpui 0.2.2 keeps `debug_bounds` entries from earlier frames.
    fn open_bar<'a>(
        cx: &'a mut TestAppContext,
        width: f32,
        last_success: &str,
        failed_regions: &[&str],
    ) -> (Entity<ContextBar>, Events, &'a mut VisualTestContext) {
        let state = loaded_state(cx);
        let bar = cx.new(|cx| ContextBar::new(state, cx));
        bar.update(cx, |bar, _| {
            bar.last_success_text = last_success.to_string();
            bar.failed_regions = failed_regions.iter().map(|r| r.to_string()).collect();
        });
        let events: Events = Default::default();
        let sink = events.clone();
        let subscribed = bar.clone();
        cx.update(|cx| {
            cx.subscribe(&subscribed, move |_, event: &ContextBarEvent, _| {
                sink.borrow_mut().push(*event)
            })
            .detach();
        });
        let root_bar = bar.clone();
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
                        cx.new(|cx| Root::new(root_bar, window, cx))
                    },
                )
            })
            .unwrap();
        let cx = VisualTestContext::from_window(window.into(), cx).into_mut();
        cx.run_until_parked();
        (bar, events, cx)
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

    fn click(cx: &mut VisualTestContext, key: &'static str) {
        let bounds = cx
            .debug_bounds(key)
            .unwrap_or_else(|| panic!("{key} should render"));
        cx.simulate_click(bounds.center(), Default::default());
        cx.run_until_parked();
    }

    #[gpui::test]
    fn wide_bar_renders_every_item_when_nothing_failed(cx: &mut TestAppContext) {
        let (_, _, cx) = open_bar(cx, 1280.0, "", &[]);

        assert_rendered(
            cx,
            &[
                "bar.aws",
                "bar.profile",
                "bar.region",
                "bar.count",
                "bar.shown",
                "bar.refresh",
                "bar.toggle-inspector",
                "bar.toggle-sidebar",
                "bar.status",
            ],
        );
        assert_absent(cx, &["bar.failure", "bar.updated"]);
    }

    #[gpui::test]
    fn updated_time_renders_once_a_refresh_has_succeeded(cx: &mut TestAppContext) {
        let (_, _, cx) = open_bar(cx, 1280.0, "Updated 12:04", &[]);

        assert_rendered(cx, &["bar.updated"]);
    }

    #[gpui::test]
    fn a_failed_region_replaces_the_status_text_with_an_amber_button(cx: &mut TestAppContext) {
        let (_, events, cx) = open_bar(cx, 1280.0, "", &["eu-central-1"]);

        assert_rendered(cx, &["bar.failure"]);
        assert_absent(cx, &["bar.status"]);

        click(cx, "bar.failure");
        assert_eq!(*events.borrow(), vec![ContextBarEvent::OpenFailureDetails]);
    }

    #[gpui::test]
    fn buttons_emit_their_events(cx: &mut TestAppContext) {
        let (_, events, cx) = open_bar(cx, 1280.0, "", &[]);

        click(cx, "bar.profile");
        click(cx, "bar.region");
        click(cx, "bar.refresh");
        click(cx, "bar.toggle-inspector");
        click(cx, "bar.toggle-sidebar");

        assert_eq!(
            *events.borrow(),
            vec![
                ContextBarEvent::OpenProfilePicker,
                ContextBarEvent::OpenRegionPicker,
                ContextBarEvent::Refresh,
                ContextBarEvent::ToggleInspector,
                ContextBarEvent::ToggleSidebar,
            ]
        );
    }

    #[gpui::test]
    fn compact_bar_keeps_only_profile_region_and_status(cx: &mut TestAppContext) {
        let (_, _, cx) = open_bar(cx, 520.0, "Updated 12:04", &[]);

        assert_rendered(cx, &["bar.profile", "bar.region", "bar.status"]);
        assert_absent(cx, &["bar.count", "bar.refresh", "bar.updated"]);
    }

    #[gpui::test]
    fn long_load_error_is_truncated_inside_a_compact_bar(cx: &mut TestAppContext) {
        let (bar, _, cx) = open_bar(cx, 520.0, "", &[]);
        bar.update(cx, |bar, cx| {
            bar.load_error_text = Some(format!("Refresh failed: {}", "Throttled ".repeat(20)));
            cx.notify();
        });
        cx.run_until_parked();

        assert_rendered(cx, &["bar.profile", "bar.region", "bar.status"]);
        let bounds = cx
            .debug_bounds("bar.load-error")
            .expect("load error should render");
        assert!(
            f32::from(bounds.right()) <= 520.0,
            "load error should stay inside the window, got right edge {:?}",
            bounds.right()
        );
        let status = cx.debug_bounds("bar.status").expect("status should render");
        assert!(
            f32::from(status.right()) <= 520.0,
            "status should stay inside the window, got right edge {:?}",
            status.right()
        );
    }

    #[gpui::test]
    fn narrow_bar_shows_counts_but_hides_updated_time(cx: &mut TestAppContext) {
        let (_, _, cx) = open_bar(cx, 700.0, "Updated 12:04", &[]);

        assert_rendered(cx, &["bar.shown"]);
        assert_absent(cx, &["bar.updated"]);
    }

    #[gpui::test]
    fn idle_main_window_records_the_status_text(cx: &mut TestAppContext) {
        let (_fleet, _main, cx) = build(State::MainIdle, 1280.0, 760.0, cx);

        let status = cx
            .update(|_, app| recorded(app, "bar.status"))
            .expect("status should be recorded");
        assert_eq!(status.text.as_deref(), Some("Ready"));
    }

    #[gpui::test]
    fn switching_from_failure_to_ready_forgets_the_failure_record(cx: &mut TestAppContext) {
        let (bar, _, cx) = open_bar(cx, 1280.0, "", &["eu-central-1"]);
        assert!(cx.update(|_, app| recorded(app, "bar.failure")).is_some());

        bar.update(cx, |bar, cx| {
            bar.failed_regions.clear();
            cx.notify();
        });
        cx.run_until_parked();

        assert_eq!(cx.update(|_, app| recorded(app, "bar.failure")), None);
        assert!(cx.update(|_, app| recorded(app, "bar.status")).is_some());
    }
}
