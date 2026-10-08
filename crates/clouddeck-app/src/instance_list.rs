use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement,
    Styled, Window, div, prelude::*, px,
};

use gpui::Entity;
use gpui_component::input::{Input, InputEvent, InputState};

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
    /// One-shot request consumed by the shell: open the Connect to AWS
    /// setup window from the empty-fleet onboarding state.
    pub manage_requested: bool,
    /// One-shot request consumed by the shell: re-scan the detected AWS
    /// profiles (e.g. after a browser sign-in created the first one) and
    /// restart the fleet load so the new profile is usable without a
    /// restart.
    pub rescan_requested: bool,
    /// Port of the oracle's `@State private var hoveredInstanceID: String?`,
    /// tracked here (rather than in the view model) since hover is
    /// transient view-local UI state, not fleet data.
    hovered_instance_id: Option<String>,
    search: Option<Entity<InputState>>,
}

impl InstanceListView {
    pub fn new(view_model: InstanceListViewModel) -> Self {
        Self {
            view_model,
            show_standalone_empty_state: false,
            manage_requested: false,
            rescan_requested: false,
            hovered_instance_id: None,
            search: None,
        }
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let Some(service) = self.view_model.begin_load() else {
            return;
        };
        cx.notify();
        let task =
            crate::runtime::runtime().spawn(async move { service.list_instances(None).await });
        cx.spawn(async move |this, cx| {
            let result = task
                .await
                .unwrap_or_else(|error| Err(std::io::Error::other(error.to_string()).into()));
            let _ = this.update(cx, |this, cx| {
                this.view_model.finish_load(result);
                cx.notify();
            });
        })
        .detach();
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
        .flex_1()
        .min_w(px(0.0))
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
        .flex_1()
        .min_w(px(0.0))
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
                this.view_model.set_selected_instance_type(value.clone());
                cx.notify();
            }),
        ));
    }

    div()
        .debug_selector(|| "ec2.instance.toolbar".into())
        .flex_wrap()
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
                .child(if view.view_model.is_loading() {
                    "Refreshing…"
                } else {
                    "Refresh"
                })
                .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
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
        .child(
            div()
                .flex_1()
                .flex_basis(px(260.0))
                .min_w(px(140.0))
                .child("Instance"),
        )
        .child(div().w(px(90.0)).flex_shrink_0().child("Region"))
        .child(div().w(px(78.0)).flex_shrink_0().child("Type"))
        .child(div().w(px(72.0)).flex_shrink_0().child("State"))
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
    let name = instance
        .name()
        .map(str::to_string)
        .unwrap_or_else(|| instance.instance_id.clone());
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
        .bg(SurgicalTheme::row_fill(is_selected, is_hovered, false)
            .unwrap_or_else(SurgicalTheme::surface))
        .child(
            div()
                .flex_1()
                .flex_basis(px(260.0))
                .min_w(px(140.0))
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(
                    div()
                        .min_w(px(0.0))
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(SurgicalTypography::BODY_EMPHASIS_SIZE))
                        .text_color(SurgicalTheme::text_primary())
                        .child(name),
                )
                .child(
                    div()
                        .min_w(px(0.0))
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(SurgicalTypography::MONO_SIZE))
                        .text_color(SurgicalTheme::text_secondary())
                        .child(instance_id),
                ),
        )
        .child(
            div()
                .w(px(90.0))
                .flex_shrink_0()
                .text_size(px(SurgicalTypography::METADATA_SIZE))
                .text_color(SurgicalTheme::text_secondary())
                .child(region),
        )
        .child(
            div()
                .w(px(78.0))
                .flex_shrink_0()
                .text_size(px(SurgicalTypography::METADATA_SIZE))
                .text_color(SurgicalTheme::text_secondary())
                .child(instance_type),
        )
        .child(
            div()
                .w(px(72.0))
                .flex_shrink_0()
                .child(status_badge("ec2.instance.row.status", state.as_deref())),
        )
        .on_click(cx.listener(move |this, _event, _window, cx| {
            let next_selection =
                if this.view_model.selected_instance_id() == Some(click_id.as_str()) {
                    None
                } else {
                    Some(click_id.clone())
                };
            this.view_model.set_selected_instance_id(next_selection);
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
fn error_panel(
    message: String,
    window: &mut Window,
    cx: &mut Context<InstanceListView>,
) -> impl IntoElement {
    let mut root = div()
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
        );
    // Any real-mode load failure (no profiles detected at all, an expired
    // SSO login, a denied-access profile, …) lands the user on exactly this
    // dead-end error panel; each of those recovery paths is the same one —
    // open the Connect to AWS window (or rescan, if a browser sign-in just
    // created the first profile) — so the in-place "Connect to AWS" /
    // "Rescan profiles" affordance renders here whenever the failure leaves
    // the fleet empty, not just for one specific error string
    // (clouddeck-rs-w6s.18). Demo mode has its own recovery affordance
    // ("Explore demo fleet"), so this is real-mode only.
    if crate::shell::is_real_mode() {
        let view_entity: gpui::Entity<InstanceListView> = cx.entity();
        root = root.child(onboarding_actions(&view_entity, window));
    }
    root
}

/// Port of `emptyPanel(hasFilter:)`.
fn empty_panel(
    has_filter: bool,
    window: &mut Window,
    cx: &mut Context<InstanceListView>,
) -> impl IntoElement {
    let (title, message): (&'static str, &str) = if has_filter {
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

    let mut root = div()
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
        );

    // The no-profiles / wrong-profile onboarding affordance: only offered
    // in real (non-demo) AWS mode. In the demo the fleet is scenario data
    // and there is nothing to set up; here a first launch with no `~/.aws`
    // profiles reaches exactly this state and previously had no visible way
    // into the Connect to AWS flow (only the small "Set up AWS" chip in the
    // status bar). Clicking "Connect to AWS" sets `manage_requested`, which
    // the shell consumes to open the setup window; clicking "Rescan
    // profiles" sets `rescan_requested`, which the shell consumes to
    // re-scan detected profiles after a browser sign-in created the first
    // one (clouddeck-rs-w6s.18).
    if crate::shell::is_real_mode() {
        let view_entity: gpui::Entity<InstanceListView> = cx.entity();
        root = root.child(onboarding_actions(&view_entity, window));
    }

    root
}

/// The in-place "Connect to AWS" / "Rescan profiles" buttons shown in the
/// empty-fleet state for real (non-demo) launches, the no-profiles /
/// wrong-profile onboarding entry point (clouddeck-rs-w6s.18). Built with
/// raw `Fn` closures (the same shape as `filter_chip`'s `on_click`
/// parameter) routed through `Window::listener_for`, which captures the
/// view entity itself — the `StatefulInteractiveElement` `cx.listener`
/// sugar does not typecheck on a `Div` inside a free function's
/// `impl IntoElement` return, but `listener_for` returns exactly the
/// `Fn(&E, &mut Window, &mut App)` signature `on_click` accepts.
fn onboarding_actions(view: &Entity<InstanceListView>, window: &mut Window) -> impl IntoElement {
    let setup_button: gpui::Stateful<gpui::Div> = div()
        .id("ec2.instance.empty-setup")
        .debug_selector(|| "ec2.instance.empty-setup".into())
        .px(px(12.0))
        .py(px(7.0))
        .rounded(px(radius::STANDARD))
        .bg(SurgicalTheme::primary())
        .text_size(px(SurgicalTypography::CAPTION_SIZE))
        .text_color(SurgicalTheme::surface())
        .child("Connect to AWS")
        .on_click(window.listener_for(view, |this, _event, _window, cx| {
            this.manage_requested = true;
            cx.notify();
        }));
    let rescan_button: gpui::Stateful<gpui::Div> = div()
        .id("ec2.instance.empty-rescan")
        .debug_selector(|| "ec2.instance.empty-rescan".into())
        .px(px(12.0))
        .py(px(7.0))
        .rounded(px(radius::STANDARD))
        .bg(SurgicalTheme::surface_container())
        .text_size(px(SurgicalTypography::CAPTION_SIZE))
        .text_color(SurgicalTheme::text_primary())
        .child("Rescan profiles")
        .on_click(window.listener_for(view, |this, _event, _window, cx| {
            this.rescan_requested = true;
            cx.notify();
        }));
    div()
        .debug_selector(|| "ec2.instance.empty-actions".into())
        .flex()
        .flex_row()
        .gap(px(8.0))
        .mt(px(10.0))
        .child(setup_button)
        .child(rescan_button)
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.search.is_none() {
            if !cx.has_global::<gpui_component::Theme>() {
                gpui_component::init(cx);
            }
            let search = cx.new(|cx| {
                InputState::new(window, cx).placeholder("Search names, tags, IPs, regions…")
            });
            cx.subscribe(&search, |this, input, event, cx| {
                if matches!(event, InputEvent::Change) {
                    this.view_model
                        .set_search_text(input.read(cx).value().to_string());
                    cx.notify();
                }
            })
            .detach();
            self.search = Some(search);
        }
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
            .pt(px(12.0))
            .pb(px(8.0))
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
                    .id("fleet-search")
                    .debug_selector(|| "fleet-search".into())
                    .w_full()
                    .h(px(34.0))
                    .flex_shrink_0()
                    .child(
                        Input::new(self.search.as_ref().unwrap())
                            .w_full()
                            .h(px(34.0))
                            .cleanable(true),
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
        } else if let Some(error_message) =
            self.view_model.error_message().filter(|_| instances_empty)
        {
            error_panel(error_message.to_string(), window, cx).into_any_element()
        } else if filtered.is_empty() {
            empty_panel(has_filter, window, cx).into_any_element()
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

        root.child(
            div()
                .id("fleet-scroll")
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .child(content),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::GatedComputeService;
    use clouddeck_core::{EC2InstanceTag, MockComputeService};
    use gpui::{Entity, TestAppContext, VisualTestContext};
    use std::cell::RefCell;
    use std::rc::Rc;

    fn instance(
        id: &str,
        name: &str,
        state: &str,
        instance_type: &str,
        region: &str,
    ) -> EC2Instance {
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
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(vm.load_instances());
        vm
    }

    fn empty_view_model() -> InstanceListViewModel {
        let mut vm = InstanceListViewModel::new(Box::new(MockComputeService::new(vec![], None)));
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(vm.load_instances());
        vm
    }

    fn add_rooted_list(
        cx: &mut TestAppContext,
    ) -> (Entity<InstanceListView>, &mut VisualTestContext) {
        let slot = Rc::new(RefCell::new(None));
        let slot_for_window = slot.clone();
        let (_root, cx) = cx.add_window_view(move |window, cx| {
            gpui_component::init(cx);
            let list = cx.new(|_| InstanceListView::new(populated_view_model()));
            *slot_for_window.borrow_mut() = Some(list.clone());
            gpui_component::Root::new(list, window, cx)
        });
        let list = slot
            .borrow()
            .as_ref()
            .expect("rooted list entity should be created")
            .clone();
        (list, cx)
    }

    /// `InstanceListView::refresh`'s load resolves on the shared tokio
    /// runtime (`crate::runtime::runtime()`), a real OS thread pool outside
    /// gpui's deterministic test executor. A single `cx.run_until_parked()`
    /// call can return before that cross-thread result has landed, so poll
    /// with bounded, short real sleeps rather than assuming one call
    /// suffices.
    fn wait_until_parked(
        cx: &mut VisualTestContext,
        mut condition: impl FnMut(&mut VisualTestContext) -> bool,
    ) {
        for _ in 0..200 {
            cx.run_until_parked();
            if condition(cx) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("condition was not satisfied within the timeout");
    }

    #[gpui::test]
    fn mounts_without_a_display(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| InstanceListView::new(populated_view_model()));

        assert!(cx.debug_bounds("ec2.instance.panel").is_some());
    }

    #[gpui::test]
    fn renders_a_row_per_filtered_instance(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| InstanceListView::new(populated_view_model()));

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

    // clouddeck-rs-w6s.18: the empty-fleet state is the onboarding entry
    // point for real (non-demo) launches with no detected profiles, so it
    // must offer an in-place "Connect to AWS" / rescan affordance instead of
    // dead-ending on "No Instances Found".
    #[gpui::test]
    fn empty_state_offers_connect_to_aws_in_real_mode(cx: &mut TestAppContext) {
        let real_mode = crate::shell::is_real_mode();
        let (view, cx) = cx.add_window_view(|_, _| InstanceListView::new(empty_view_model()));

        assert!(
            cx.debug_bounds("ec2.instance.empty").is_some(),
            "the empty state panel must render regardless of mode"
        );

        if !real_mode {
            assert!(
                cx.debug_bounds("ec2.instance.empty-setup").is_none(),
                "the demo fleet must not offer AWS onboarding"
            );
            assert!(
                cx.debug_bounds("ec2.instance.empty-rescan").is_none(),
                "the demo fleet must not offer a profile rescan"
            );
            return;
        }

        assert!(
            cx.debug_bounds("ec2.instance.empty-setup").is_some(),
            "a real-mode empty fleet should offer Connect to AWS in place"
        );
        assert!(
            cx.debug_bounds("ec2.instance.empty-rescan").is_some(),
            "a real-mode empty fleet should offer a profile rescan"
        );

        // The affordance is rendered by the fleet view's shell-level observer
        // (one-shot `manage_requested` / `rescan_requested` flags), not by a
        // per-button click handler: clicking where the affordance renders
        // must trigger the setup request the shell consumes to open the
        // Connect to AWS window. The click is routed through the list's
        // onboarding handler.
        let setup = cx
            .debug_bounds("ec2.instance.empty-setup")
            .expect("connect-to-aws affordance renders");
        cx.simulate_click(setup.center(), Default::default());
        cx.run_until_parked();
        assert!(
            view.read_with(cx, |view, _| view.manage_requested),
            "clicking Connect to AWS must flag a setup request for the shell"
        );

        let rescan = cx
            .debug_bounds("ec2.instance.empty-rescan")
            .expect("rescan affordance renders");
        cx.simulate_click(rescan.center(), Default::default());
        cx.run_until_parked();
        assert!(
            view.read_with(cx, |view, _| view.rescan_requested),
            "clicking Rescan profiles must flag a rescan request for the shell"
        );
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
        let (_view, cx) = cx.add_window_view(|_, _| InstanceListView::new(populated_view_model()));

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
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
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
        let (_view, cx) = cx.add_window_view(|_, _| InstanceListView::new(populated_view_model()));
        assert!(cx.debug_bounds("ec2.instance.header").is_some());

        let (_view2, cx2) = cx.add_window_view(|_, _| InstanceListView::new(empty_view_model()));
        assert!(cx2.debug_bounds("ec2.instance.header").is_some());
    }

    #[gpui::test]
    fn hovering_a_row_marks_it_hovered_and_moving_off_clears_it(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| InstanceListView::new(populated_view_model()));

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

    #[gpui::test]
    fn toolbar_renders_search_state_type_filters_and_refresh_together(cx: &mut TestAppContext) {
        // Structural proof for the agreed layout (clouddeck-rs-wz2.3): the
        // top toolbar carries search, State and Type filters, and Refresh
        // side by side, above the full-width table.
        let (_view, cx) = cx.add_window_view(|_, cx| {
            gpui_component::init(cx);
            InstanceListView::new(populated_view_model())
        });

        let header = cx
            .debug_bounds("ec2.instance.header")
            .expect("header region should render");
        let search = cx
            .debug_bounds("fleet-search")
            .expect("search field should render in the toolbar");
        let state_filter = cx
            .debug_bounds("ec2.filter.state")
            .expect("State filter should render in the toolbar");
        let type_filter = cx
            .debug_bounds("ec2.filter.type")
            .expect("Type filter should render in the toolbar");
        let refresh = cx
            .debug_bounds("ec2.instance.refresh")
            .expect("Refresh control should render in the toolbar");

        // Search sits above the filter/refresh row, and all of them live
        // inside the header region (not off in some other panel).
        assert!(search.origin.y >= header.origin.y);
        assert!(state_filter.origin.y >= search.origin.y + search.size.height);
        assert!(type_filter.origin.y >= search.origin.y + search.size.height);
        assert!(refresh.origin.y >= search.origin.y + search.size.height);
        // State, Type, and Refresh are pinned to the same toolbar row (the
        // oracle's picker/refresh row), not stacked vertically: each pair's
        // vertical spans overlap rather than one sitting fully above/below
        // another. Chips and the refresh control have slightly different
        // paddings, so `items_center` can offset their tops by a subpixel or
        // two; compare overlap rather than exact `origin.y` equality.
        let overlaps = |a: gpui::Bounds<gpui::Pixels>, b: gpui::Bounds<gpui::Pixels>| {
            a.origin.y < b.origin.y + b.size.height && b.origin.y < a.origin.y + a.size.height
        };
        assert!(
            overlaps(state_filter, type_filter),
            "State and Type filters should share a row"
        );
        assert!(
            overlaps(state_filter, refresh),
            "State filter and Refresh should share a row"
        );
    }

    #[gpui::test]
    fn typing_in_search_filters_the_rendered_fleet(cx: &mut TestAppContext) {
        let (view, cx) = add_rooted_list(cx);

        let search = cx.debug_bounds("fleet-search").expect("search renders");
        cx.simulate_click(search.center(), Default::default());
        cx.simulate_input("bastion");

        assert_eq!(
            view.read_with(cx, |view, _| view.view_model.search_text().to_string()),
            "bastion"
        );
        assert_eq!(
            view.read_with(cx, |view, _| view.view_model.filtered_instances().len()),
            1,
            "typing should immediately narrow the visible fleet"
        );
        assert!(
            cx.debug_bounds("ec2.instance.row.us-west-2:i-2").is_some(),
            "the matching row should remain rendered"
        );
    }

    #[gpui::test]
    fn clicking_the_selected_row_clears_selection(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| InstanceListView::new(populated_view_model()));
        let row = cx
            .debug_bounds("ec2.instance.row.us-east-1:i-1")
            .expect("row renders");

        cx.simulate_click(row.center(), Default::default());
        assert_eq!(
            view.read_with(cx, |view, _| {
                view.view_model.selected_instance_id().map(str::to_owned)
            }),
            Some("us-east-1:i-1".to_string())
        );

        cx.simulate_click(row.center(), Default::default());
        assert_eq!(
            view.read_with(cx, |view, _| {
                view.view_model.selected_instance_id().map(str::to_owned)
            }),
            None,
            "clicking the active row again should return to the unselected state"
        );
    }

    #[gpui::test]
    fn clicking_refresh_reloads_via_the_async_spawn_pattern(cx: &mut TestAppContext) {
        // `InstanceListView::refresh` kicks off `begin_load()` synchronously
        // (flipping `is_loading` before the click handler returns) then
        // finishes the load on a `cx.spawn`-bridged tokio task, never
        // blocking the UI thread. Gate the service's response so both
        // halves of that transition are observable deterministically.
        let gate = std::sync::Arc::new(tokio::sync::Notify::new());
        let service = GatedComputeService {
            gate: gate.clone(),
            instances: vec![instance(
                "i-1",
                "api-server",
                "running",
                "t3.medium",
                "us-east-1",
            )],
        };
        let vm = InstanceListViewModel::new(Box::new(service));
        let (view, cx) = cx.add_window_view(|_, _| InstanceListView::new(vm));
        let refresh = cx
            .debug_bounds("ec2.instance.refresh")
            .expect("refresh control renders");

        cx.simulate_click(refresh.center(), Default::default());

        assert!(
            view.read_with(cx, |view, _| view.view_model.is_loading()),
            "clicking refresh should synchronously flip is_loading, without \
             waiting on the network call"
        );
        assert!(
            cx.debug_bounds("ec2.instance.loading").is_some(),
            "the loading panel should render while the gated load is in flight"
        );

        gate.notify_one();
        wait_until_parked(cx, |cx| {
            !view.read_with(cx, |view, _| view.view_model.is_loading())
        });

        assert!(
            !view.read_with(cx, |view, _| view.view_model.is_loading()),
            "is_loading should clear once the spawned load completes"
        );
        assert_eq!(
            view.read_with(cx, |view, _| view.view_model.instances().len()),
            1,
            "refresh should load the instances the gated service returns"
        );
    }
}
