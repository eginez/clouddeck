//! Port of `InstanceListView.swift` (the Swift CloudDeck oracle's
//! scrollable EC2 instance table) to gpui.
//!
//! Driven entirely by an [`InstanceListViewModel`]: a header (title + a
//! "N visible" count once instances are loaded), an optional
//! partial-failure banner, a state/type filter chip row + refresh button
//! standing in for the oracle's toolbar `Picker`s (gpui has no native menu
//! picker; see the `filter_row` doc comment below), and content that
//! switches between a loading panel, an error panel, an empty-state
//! panel, and the instance table depending on view-model state — matching
//! the oracle's `@ViewBuilder private var content` branching exactly. See
//! `~/src/cloudDeck/main/Sources/CloudDeck/Views/EC2/InstanceListView.swift`
//! for the oracle this was ported from.
//!
//! The refresh button and filter chips mutate view-model state
//! synchronously (`set_selected_state`/`set_selected_instance_type`) but
//! do **not** call the async `refresh`/`load_instances` themselves: this
//! codebase has no established pattern yet for driving an async
//! view-model call from a gpui click handler (see
//! `crates/clouddeck-app/src/view_models/instance_list.rs`). Wiring the
//! refresh button and initial `.task { loadInstances() }` load to the
//! real async calls is the EC2-tab composition bead's job
//! (`clouddeck-rs-w6s.14`), which owns a live `AppShell`/`cx` to spawn
//! that work from.
//!
//! Row hover state is tracked here (`InstanceListView::hovered_instance_id`)
//! via gpui's `on_hover`, mirroring the oracle's `@State private var
//! hoveredInstanceID` and feeding into `SurgicalTheme::row_fill`'s
//! `hovered` argument exactly like the oracle's `rowFill(selected:
//! hovered:)` call.
//!
//! The oracle's `.searchable(text: $viewModel.searchText, ...)` search
//! field is **not** ported here: `InstanceListViewModel::search_text`
//! already exists and is applied by `filtered_instances()`, but this
//! codebase has no established gpui text-input widget anywhere yet (no
//! `on_hover`-adjacent `on_key_down`/IME-editable element is used by any
//! other ported view — see e.g. `account_profile_picker.rs` and
//! `quick_picker.rs`, which have search-filtering *logic* but render no
//! actual editable text field). Adding a search box here should wait
//! until such a pattern is established for the app (tracked as a
//! discovered-from follow-up), rather than growing an ad hoc one-off
//! implementation in this view.

use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement,
    Styled, Window, div, prelude::*, px,
};

use crate::status_badge::status_badge;
use crate::theme::{SurgicalTheme, SurgicalTypography, radius};
use crate::view_models::InstanceListViewModel;
use clouddeck_core::EC2Instance;

/// Port of `InstanceListView`. Owns the view-model for the EC2 fleet list
/// and renders it; `show_standalone_empty_state` mirrors the oracle's
/// `showStandaloneEmptyState` flag (used when this view is shown without a
/// paired detail view, so it renders its own "Select an Instance" prompt
/// below the table).
pub struct InstanceListView {
    pub view_model: InstanceListViewModel,
    pub show_standalone_empty_state: bool,
    /// Port of the oracle's `@State private var hoveredInstanceID: String?`,
    /// tracked here (rather than in the view model) since hover is
    /// transient view-local UI state, not fleet data.
    hovered_instance_id: Option<String>,
}

impl InstanceListView {
    pub fn new(view_model: InstanceListViewModel) -> Self {
        Self {
            view_model,
            show_standalone_empty_state: false,
            hovered_instance_id: None,
        }
    }

    /// Port of `hasAnyFilter`: whether any search/state/type filter is
    /// currently active.
    fn has_any_filter(&self) -> bool {
        !self.view_model.search_text().trim().is_empty()
            || !self.view_model.selected_state().is_empty()
            || !self.view_model.selected_instance_type().is_empty()
    }
}

fn section_header(title: &'static str) -> impl IntoElement {
    div()
        .text_size(px(SurgicalTypography::LABEL_SIZE))
        .text_color(SurgicalTheme::text_secondary())
        .child(title.to_uppercase())
}

/// Port of the toolbar's state/type `Picker`s + refresh `Button`. gpui has
/// no native menu-picker widget, so each available value is rendered as a
/// standalone clickable chip (selected chip highlighted via
/// `SurgicalTheme::row_fill`), plus an explicit "All" chip to clear the
/// filter — functionally equivalent to the oracle's picker (pick one value
/// or none) without requiring a popover menu.
fn filter_row(view: &mut InstanceListView, cx: &mut Context<InstanceListView>) -> impl IntoElement {
    let selected_state = view.view_model.selected_state().to_string();
    let selected_type = view.view_model.selected_instance_type().to_string();
    let states = view.view_model.available_states();
    let types = view.view_model.available_instance_types();

    let mut state_chips = div()
        .debug_selector(|| "ec2.filter.state".into())
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(px(6.0))
        .child(filter_chip(
            "ec2.filter.state.all",
            "All States",
            selected_state.is_empty(),
            cx.listener(|this, _event, _window, cx| {
                this.view_model.set_selected_state(String::new());
                cx.notify();
            }),
        ));
    for state in states {
        let is_selected = state == selected_state;
        let value = state.clone();
        let label = capitalize_first(&state);
        state_chips = state_chips.child(filter_chip(
            "ec2.filter.state.value",
            label,
            is_selected,
            cx.listener(move |this, _event, _window, cx| {
                this.view_model.set_selected_state(value.clone());
                cx.notify();
            }),
        ));
    }

    let mut type_chips = div()
        .debug_selector(|| "ec2.filter.type".into())
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(px(6.0))
        .child(filter_chip(
            "ec2.filter.type.all",
            "All Types",
            selected_type.is_empty(),
            cx.listener(|this, _event, _window, cx| {
                this.view_model.set_selected_instance_type(String::new());
                cx.notify();
            }),
        ));
    for instance_type in types {
        let is_selected = instance_type == selected_type;
        let value = instance_type.clone();
        type_chips = type_chips.child(filter_chip(
            "ec2.filter.type.value",
            instance_type,
            is_selected,
            cx.listener(move |this, _event, _window, cx| {
                this.view_model
                    .set_selected_instance_type(value.clone());
                cx.notify();
            }),
        ));
    }

    div()
        .debug_selector(|| "ec2.instance.toolbar".into())
        .flex()
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .child(state_chips)
        .child(type_chips)
        .child(
            div()
                .id("ec2.instance.refresh")
                .debug_selector(|| "ec2.instance.refresh".into())
                .px(px(10.0))
                .py(px(6.0))
                .rounded(px(radius::STANDARD))
                .bg(SurgicalTheme::surface_container())
                .text_size(px(SurgicalTypography::CAPTION_SIZE))
                .text_color(SurgicalTheme::text_primary())
                .child("Refresh"),
        )
}

fn filter_chip(
    selector: &'static str,
    label: impl Into<gpui::SharedString>,
    selected: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let background = SurgicalTheme::row_fill(selected, false, false)
        .unwrap_or_else(SurgicalTheme::surface_container);
    let label: gpui::SharedString = label.into();

    div()
        .id(gpui::SharedString::from(format!("{selector}-{label}")))
        .debug_selector(move || selector.into())
        .px(px(10.0))
        .py(px(5.0))
        .rounded(px(radius::STANDARD))
        .bg(background)
        .text_size(px(SurgicalTypography::CAPTION_SIZE))
        .text_color(if selected {
            SurgicalTheme::primary_dim()
        } else {
            SurgicalTheme::text_secondary()
        })
        .child(label)
        .on_click(on_click)
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Port of `partialFailureBanner(summary:)`.
fn partial_failure_banner(summary: String) -> impl IntoElement {
    div()
        .debug_selector(|| "ec2.instance.partial-failure-banner".into())
        .flex()
        .flex_row()
        .items_center()
        .gap(px(10.0))
        .px(px(20.0))
        .py(px(10.0))
        .bg(SurgicalTheme::surface_low())
        .child(
            div()
                .text_size(px(SurgicalTypography::CAPTION_SIZE))
                .text_color(SurgicalTheme::text_primary())
                .child(summary),
        )
}

/// Port of `tableHeader`.
fn table_header() -> impl IntoElement {
    div()
        .debug_selector(|| "ec2.instance.table-header".into())
        .flex()
        .flex_row()
        .gap(px(12.0))
        .px(px(12.0))
        .py(px(8.0))
        .rounded(px(radius::STANDARD))
        .bg(SurgicalTheme::surface_high())
        .text_size(px(SurgicalTypography::LABEL_SIZE))
        .text_color(SurgicalTheme::text_secondary())
        .child(div().flex_1().child("Instance"))
        .child(div().w(px(110.0)).child("Region"))
        .child(div().w(px(96.0)).child("Type"))
        .child(div().w(px(92.0)).child("State"))
}

/// Port of `row(for:)`.
fn instance_row(
    instance: &EC2Instance,
    is_selected: bool,
    is_hovered: bool,
    cx: &mut Context<InstanceListView>,
) -> impl IntoElement {
    let selector: &'static str = "ec2.instance.row";
    let row_id = instance.id();
    let click_id = row_id.clone();
    let hover_id = row_id.clone();
    let name = instance.name().map(str::to_string).unwrap_or_else(|| instance.instance_id.clone());
    let instance_id = instance.instance_id.clone();
    let region = instance.region.clone().unwrap_or_else(|| "—".to_string());
    let instance_type = instance
        .instance_type
        .clone()
        .unwrap_or_else(|| "—".to_string());
    let state = instance.state.clone();

    div()
        .id(gpui::SharedString::from(format!("ec2.instance.{row_id}")))
        .debug_selector(move || format!("{selector}.{row_id}"))
        .flex()
        .flex_row()
        .gap(px(12.0))
        .px(px(12.0))
        .py(px(11.0))
        .rounded(px(radius::STANDARD))
        .bg(SurgicalTheme::row_fill(is_selected, is_hovered, false).unwrap_or_else(SurgicalTheme::surface))
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(
                    div()
                        .text_size(px(SurgicalTypography::BODY_EMPHASIS_SIZE))
                        .text_color(SurgicalTheme::text_primary())
                        .child(name),
                )
                .child(
                    div()
                        .text_size(px(SurgicalTypography::MONO_SIZE))
                        .text_color(SurgicalTheme::text_secondary())
                        .child(instance_id),
                ),
        )
        .child(
            div()
                .w(px(110.0))
                .text_size(px(SurgicalTypography::METADATA_SIZE))
                .text_color(SurgicalTheme::text_secondary())
                .child(region),
        )
        .child(
            div()
                .w(px(96.0))
                .text_size(px(SurgicalTypography::METADATA_SIZE))
                .text_color(SurgicalTheme::text_secondary())
                .child(instance_type),
        )
        .child(div().w(px(92.0)).child(status_badge(
            "ec2.instance.row.status",
            state.as_deref(),
        )))
        .on_click(cx.listener(move |this, _event, _window, cx| {
            this.view_model
                .set_selected_instance_id(Some(click_id.clone()));
            cx.notify();
        }))
        .on_hover(cx.listener(move |this, is_hovering, _window, cx| {
            this.hovered_instance_id = if *is_hovering {
                Some(hover_id.clone())
            } else {
                None
            };
            cx.notify();
        }))
}

/// Port of `loadingPanel`.
fn loading_panel() -> impl IntoElement {
    div()
        .debug_selector(|| "ec2.instance.loading".into())
        .flex()
        .flex_col()
        .gap(px(4.0))
        .px(px(20.0))
        .py(px(16.0))
        .bg(SurgicalTheme::surface_low())
        .child(
            div()
                .text_size(px(SurgicalTypography::BODY_EMPHASIS_SIZE))
                .text_color(SurgicalTheme::text_primary())
                .child("Loading Instances"),
        )
        .child(
            div()
                .text_size(px(SurgicalTypography::CAPTION_SIZE))
                .text_color(SurgicalTheme::text_secondary())
                .child("Fetching EC2 instances for the selected AWS profile and region."),
        )
}

/// Port of `errorPanel(message:)`.
fn error_panel(message: String) -> impl IntoElement {
    div()
        .debug_selector(|| "ec2.instance.error".into())
        .flex()
        .flex_col()
        .gap(px(8.0))
        .px(px(20.0))
        .py(px(16.0))
        .bg(SurgicalTheme::surface_low())
        .child(
            div()
                .text_size(px(SurgicalTypography::BODY_EMPHASIS_SIZE))
                .text_color(SurgicalTheme::text_primary())
                .child("Could Not Load Instances"),
        )
        .child(
            div()
                .text_size(px(SurgicalTypography::CAPTION_SIZE))
                .text_color(SurgicalTheme::text_secondary())
                .child(message),
        )
}

/// Port of `emptyPanel(hasFilter:)`.
fn empty_panel(has_filter: bool) -> impl IntoElement {
    let (title, message): (&'static str, &'static str) = if has_filter {
        (
            "No Matching Instances",
            "Adjust your search or filter values to widen the current fleet view.",
        )
    } else {
        (
            "No Instances Found",
            "No EC2 instances are visible for the selected account and region scope.",
        )
    };

    div()
        .debug_selector(|| "ec2.instance.empty".into())
        .flex()
        .flex_col()
        .gap(px(4.0))
        .px(px(20.0))
        .py(px(16.0))
        .bg(SurgicalTheme::surface_low())
        .child(
            div()
                .text_size(px(SurgicalTypography::BODY_EMPHASIS_SIZE))
                .text_color(SurgicalTheme::text_primary())
                .child(title),
        )
        .child(
            div()
                .text_size(px(SurgicalTypography::CAPTION_SIZE))
                .text_color(SurgicalTheme::text_secondary())
                .child(message),
        )
}

/// Port of `standaloneInspectorPrompt`.
fn standalone_inspector_prompt() -> impl IntoElement {
    div()
        .debug_selector(|| "ec2.instance.standalone-inspector-prompt".into())
        .flex()
        .flex_col()
        .gap(px(14.0))
        .px(px(20.0))
        .pt(px(8.0))
        .pb(px(24.0))
        .bg(SurgicalTheme::workspace_canvas())
        .child(section_header("Inspector"))
        .child(
            div()
                .text_size(px(SurgicalTypography::VIEW_TITLE_SIZE))
                .text_color(SurgicalTheme::text_primary())
                .child("Select an Instance"),
        )
        .child(
            div()
                .text_size(px(SurgicalTypography::BODY_SIZE))
                .text_color(SurgicalTheme::text_secondary())
                .child(
                    "Choose an EC2 instance to inspect fleet metadata for the current account and region.",
                ),
        )
}

impl Render for InstanceListView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let instances_empty = self.view_model.instances().is_empty();
        let filtered = self.view_model.filtered_instances();
        let selected_id = self.view_model.selected_instance_id().map(str::to_string);
        let hovered_id = self.hovered_instance_id.clone();
        let has_filter = self.has_any_filter();
        let partial_failure_summary = self.view_model.partial_failure_summary();

        let header = div()
            .debug_selector(|| "ec2.instance.header".into())
            .flex()
            .flex_col()
            .gap(px(4.0))
            .px(px(20.0))
            .pt(px(8.0))
            .pb(px(4.0))
            .bg(SurgicalTheme::workspace_canvas())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .items_center()
                    .child(section_header("EC2 Fleet"))
                    .when(!instances_empty, |el| {
                        el.child(
                            div()
                                .text_size(px(SurgicalTypography::CAPTION_SIZE))
                                .text_color(SurgicalTheme::text_secondary())
                                .child(format!("{} visible", filtered.len())),
                        )
                    }),
            )
            .child(
                div()
                    .text_size(px(SurgicalTypography::BODY_SIZE))
                    .text_color(SurgicalTheme::text_secondary())
                    .child(
                        "Inspect instance state, identity, and configuration for the current account and region.",
                    ),
            )
            .child(filter_row(self, cx));

        let mut root = div()
            .debug_selector(|| "ec2.instance.panel".into())
            .flex()
            .flex_col()
            .size_full()
            .bg(SurgicalTheme::workspace_canvas())
            .child(header);

        if let Some(summary) = partial_failure_summary {
            root = root.child(partial_failure_banner(summary));
        }

        let content = if self.view_model.is_loading() && instances_empty {
            loading_panel().into_any_element()
        } else if let Some(error_message) = self
            .view_model
            .error_message()
            .filter(|_| instances_empty)
        {
            error_panel(error_message.to_string()).into_any_element()
        } else if filtered.is_empty() {
            empty_panel(has_filter).into_any_element()
        } else {
            let mut list = div()
                .debug_selector(|| "ec2.instance.list".into())
                .flex()
                .flex_col()
                .gap(px(2.0))
                .px(px(12.0))
                .pb(px(8.0))
                .child(table_header());
            for instance in &filtered {
                let is_selected = selected_id.as_deref() == Some(instance.id().as_str());
                let is_hovered = hovered_id.as_deref() == Some(instance.id().as_str());
                list = list.child(instance_row(instance, is_selected, is_hovered, cx));
            }

            if self.show_standalone_empty_state {
                div()
                    .flex()
                    .flex_col()
                    .child(list)
                    .child(standalone_inspector_prompt())
                    .into_any_element()
            } else {
                list.into_any_element()
            }
        };

        root.child(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clouddeck_core::{EC2InstanceTag, MockComputeService};
    use gpui::TestAppContext;

    fn instance(id: &str, name: &str, state: &str, instance_type: &str, region: &str) -> EC2Instance {
        EC2Instance {
            instance_id: id.to_string(),
            region: Some(region.to_string()),
            instance_type: Some(instance_type.to_string()),
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
        }
    }

    fn populated_view_model() -> InstanceListViewModel {
        let instances = vec![
            instance("i-1", "api-server", "running", "t3.medium", "us-east-1"),
            instance("i-2", "bastion", "stopped", "t3.small", "us-west-2"),
        ];
        let mut vm = InstanceListViewModel::new(Box::new(MockComputeService::new(instances, None)));
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        rt.block_on(vm.load_instances());
        vm
    }

    fn empty_view_model() -> InstanceListViewModel {
        let mut vm = InstanceListViewModel::new(Box::new(MockComputeService::new(vec![], None)));
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        rt.block_on(vm.load_instances());
        vm
    }

    #[gpui::test]
    fn mounts_without_a_display(cx: &mut TestAppContext) {
        let (_view, cx) =
            cx.add_window_view(|_, _| InstanceListView::new(populated_view_model()));

        assert!(cx.debug_bounds("ec2.instance.panel").is_some());
    }

    #[gpui::test]
    fn renders_a_row_per_filtered_instance(cx: &mut TestAppContext) {
        let (_view, cx) =
            cx.add_window_view(|_, _| InstanceListView::new(populated_view_model()));

        assert!(cx.debug_bounds("ec2.instance.list").is_some());
        assert!(cx.debug_bounds("ec2.instance.table-header").is_some());
        // `populated_view_model` fixture instances resolve to these
        // `EC2Instance::id()` values (`{region}:{instance_id}`).
        assert!(
            cx.debug_bounds("ec2.instance.row.us-east-1:i-1").is_some(),
            "row for us-east-1:i-1 should render"
        );
        assert!(
            cx.debug_bounds("ec2.instance.row.us-west-2:i-2").is_some(),
            "row for us-west-2:i-2 should render"
        );
        assert!(cx.debug_bounds("ec2.instance.empty").is_none());
    }

    #[gpui::test]
    fn renders_empty_state_when_there_are_no_instances(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| InstanceListView::new(empty_view_model()));

        assert!(cx.debug_bounds("ec2.instance.empty").is_some());
        assert!(cx.debug_bounds("ec2.instance.list").is_none());
    }

    #[gpui::test]
    fn renders_standalone_empty_state_when_flag_is_set(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| {
            let mut view = InstanceListView::new(populated_view_model());
            view.show_standalone_empty_state = true;
            view
        });

        assert!(
            cx.debug_bounds("ec2.instance.standalone-inspector-prompt")
                .is_some()
        );
    }

    #[gpui::test]
    fn does_not_render_standalone_prompt_by_default(cx: &mut TestAppContext) {
        let (_view, cx) =
            cx.add_window_view(|_, _| InstanceListView::new(populated_view_model()));

        assert!(
            cx.debug_bounds("ec2.instance.standalone-inspector-prompt")
                .is_none()
        );
    }

    #[gpui::test]
    fn renders_error_panel_when_instances_failed_to_load(cx: &mut TestAppContext) {
        let mut vm = InstanceListViewModel::new(Box::new(MockComputeService::new(
            vec![],
            Some(Box::new(std::io::Error::other("boom"))),
        )));
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        rt.block_on(vm.load_instances());
        assert!(vm.error_message().is_some());

        let (_view, cx) = cx.add_window_view(|_, _| InstanceListView::new(vm));

        assert!(cx.debug_bounds("ec2.instance.error").is_some());
        assert!(cx.debug_bounds("ec2.instance.list").is_none());
    }

    #[gpui::test]
    fn renders_status_badge_per_row_colored_by_state(cx: &mut TestAppContext) {
        let vm = populated_view_model();
        let (_view, cx) = cx.add_window_view(|_, _| InstanceListView::new(vm));

        assert!(cx.debug_bounds("ec2.instance.row.status").is_some());
    }

    #[gpui::test]
    fn header_shows_visible_count_only_when_instances_are_loaded(cx: &mut TestAppContext) {
        let (_view, cx) =
            cx.add_window_view(|_, _| InstanceListView::new(populated_view_model()));
        assert!(cx.debug_bounds("ec2.instance.header").is_some());

        let (_view2, cx2) = cx.add_window_view(|_, _| InstanceListView::new(empty_view_model()));
        assert!(cx2.debug_bounds("ec2.instance.header").is_some());
    }

    #[gpui::test]
    fn hovering_a_row_marks_it_hovered_and_moving_off_clears_it(cx: &mut TestAppContext) {
        let (view, cx) =
            cx.add_window_view(|_, _| InstanceListView::new(populated_view_model()));

        let bounds = cx
            .debug_bounds("ec2.instance.row.us-east-1:i-1")
            .expect("row should render");

        assert!(view.read_with(cx, |view, _| view.hovered_instance_id.is_none()));

        cx.simulate_mouse_move(bounds.center(), None, gpui::Modifiers::default());
        assert_eq!(
            view.read_with(cx, |view, _| view.hovered_instance_id.clone()),
            Some("us-east-1:i-1".to_string())
        );

        // Move somewhere clearly outside the row (top-left corner of the
        // window) so the row's hover callback fires with `false`.
        cx.simulate_mouse_move(
            gpui::point(px(0.0), px(0.0)),
            None,
            gpui::Modifiers::default(),
        );
        assert_eq!(
            view.read_with(cx, |view, _| view.hovered_instance_id.clone()),
            None
        );
    }
}
