//! Port of `ContentView.swift` (the Swift CloudDeck oracle's top-level app
//! shell layout): a `NavigationSplitView`-style sidebar/main-content region
//! docked above a bottom status bar.
//!
//! The sidebar, main content, and status bar regions now mount the real
//! ported views (`Sidebar`, `EC2Tab`, `CloudContextStatusBar`) rather than
//! empty placeholder panels; this module owns the region layout and wires
//! a `ComputeService` into the EC2 tab's view-model. See
//! `~/src/cloudDeck/main/Sources/CloudDeck/Views/ContentView.swift` for the
//! oracle this was ported from.

use gpui::{
    Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div, prelude::*, px,
};

use crate::ec2_tab::EC2Tab;
use crate::sidebar::Sidebar;
use crate::status_bar::CloudContextStatusBar;
use crate::theme::SurgicalTheme;
use crate::view_models::InstanceListViewModel;
use clouddeck_core::{
    ComputeService, EC2Instance, EC2InstanceTag, FaultInjectingComputeService, MockComputeService,
};

/// A small set of sample EC2 instances used to give the fleet UI real
/// (if mock) data to render on startup, until a live `ComputeService`
/// (e.g. an AWS-backed one) is wired in by a follow-on bead.
fn sample_instances() -> Vec<EC2Instance> {
    fn instance(id: &str, name: &str, region: &str, state: &str, kind: &str) -> EC2Instance {
        EC2Instance {
            instance_id: id.to_string(),
            region: Some(region.to_string()),
            instance_type: Some(kind.to_string()),
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

    vec![
        instance(
            "i-fleet-001",
            "api-server",
            "us-east-1",
            "running",
            "t3.medium",
        ),
        instance("i-fleet-002", "bastion", "us-east-1", "stopped", "t3.micro"),
        instance(
            "i-fleet-003",
            "worker-1",
            "us-west-2",
            "running",
            "m5.large",
        ),
    ]
}

/// Builds the `ComputeService` used to back the EC2 tab's instance list.
///
/// Wraps a fixture-backed `MockComputeService` in a `FaultInjectingComputeService`
/// (passthrough by default, i.e. no faults injected) so the fault-injection
/// seam established for the compute service layer is exercised here rather
/// than bypassed; swapping this out for a real AWS-backed service is a
/// separate follow-on bead.
fn default_compute_service() -> Box<dyn ComputeService + Send + Sync> {
    Box::new(FaultInjectingComputeService::new(MockComputeService::new(
        sample_instances(),
        None,
    )))
}

/// Top-level app shell: sidebar | main content, with a status bar docked
/// at the bottom. Mirrors `ContentView`'s `VStack { NavigationSplitView {
/// sidebar } detail: { main } ; statusBar }` structure.
pub struct AppShell {
    sidebar: Entity<Sidebar>,
    ec2_tab: Entity<EC2Tab>,
    status_bar: Entity<CloudContextStatusBar>,
}

impl AppShell {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let sidebar = cx.new(|_| Sidebar);

        let mut view_model = InstanceListViewModel::new(default_compute_service());
        // The fixture-backed service resolves synchronously, so blocking
        // once at startup to populate the initial instance list is cheap
        // and avoids inventing a new async-spawn-into-entity pattern for
        // this first wired version; a live/remote `ComputeService` wiring
        // a real async refresh path is a follow-on bead's concern.
        cx.background_executor().block(view_model.load_instances());
        let ec2_tab = cx.new(|cx| EC2Tab::new(view_model, cx));

        let status_bar = cx.new(|_| CloudContextStatusBar::default());

        Self {
            sidebar,
            ec2_tab,
            status_bar,
        }
    }
}

impl Render for AppShell {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .debug_selector(|| "app-shell".into())
            .flex()
            .flex_col()
            .size_full()
            .bg(SurgicalTheme::workspace_canvas())
            .child(
                // NavigationSplitView row: sidebar next to main content.
                div()
                    .debug_selector(|| "shell-split".into())
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(
                        // Sidebar: `SidebarView`, `minWidth: 220,
                        // idealWidth: 248` in the oracle.
                        div()
                            .debug_selector(|| "shell-sidebar".into())
                            .flex_shrink_0()
                            .w(px(248.0))
                            .min_w(px(220.0))
                            .h_full()
                            .bg(SurgicalTheme::chrome())
                            .child(self.sidebar.clone()),
                    )
                    .child(
                        // Main content: `EC2Tab`, `minWidth: 700,
                        // minHeight: 480` in the oracle.
                        div()
                            .debug_selector(|| "shell-main-content".into())
                            .flex_1()
                            .min_w(px(700.0))
                            .min_h(px(480.0))
                            .h_full()
                            .bg(SurgicalTheme::workspace_canvas())
                            .child(self.ec2_tab.clone()),
                    ),
            )
            .child(
                // Bottom status bar: `CloudContextStatusBar`. A nominal
                // height keeps this region visibly proportioned; it isn't
                // collapsed to zero.
                div()
                    .debug_selector(|| "shell-status-bar".into())
                    .flex_shrink_0()
                    .w_full()
                    .h(px(28.0))
                    .bg(SurgicalTheme::chrome_dark())
                    .child(self.status_bar.clone()),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    fn mounts_without_a_display(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, cx| AppShell::new(cx));

        assert!(
            cx.debug_bounds("app-shell").is_some(),
            "app-shell root should render"
        );
    }

    #[gpui::test]
    fn renders_all_three_regions_with_real_child_components(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, cx| AppShell::new(cx));

        assert!(
            cx.debug_bounds("shell-sidebar").is_some(),
            "sidebar region should render"
        );
        assert!(
            cx.debug_bounds("shell-main-content").is_some(),
            "main content region should render"
        );
        assert!(
            cx.debug_bounds("shell-status-bar").is_some(),
            "status bar region should render"
        );

        // The regions mount the real ported components, not empty
        // placeholder panels.
        assert!(
            cx.debug_bounds("sidebar-root").is_some(),
            "the real Sidebar should be mounted inside shell-sidebar"
        );
        assert!(
            cx.debug_bounds("ec2.tab.list-only")
                .or(cx.debug_bounds("ec2.tab.split"))
                .is_some(),
            "the real EC2Tab should be mounted inside shell-main-content"
        );
        assert!(
            cx.debug_bounds("statusbar.root").is_some(),
            "the real CloudContextStatusBar should be mounted inside shell-status-bar"
        );
    }

    #[gpui::test]
    fn ec2_tab_shows_the_seeded_sample_instances(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, cx| AppShell::new(cx));

        assert!(
            cx.debug_bounds("ec2.tab.list-only").is_some(),
            "no instance is selected initially, so the list-only layout should render"
        );
        assert!(
            cx.debug_bounds("ec2.instance.row.us-east-1:i-fleet-001")
                .is_some(),
            "a seeded sample instance row should render"
        );
    }

    #[gpui::test]
    fn sidebar_and_main_content_are_laid_out_side_by_side(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, cx| AppShell::new(cx));

        let sidebar = cx
            .debug_bounds("shell-sidebar")
            .expect("sidebar should render");
        let main_content = cx
            .debug_bounds("shell-main-content")
            .expect("main content should render");
        let status_bar = cx
            .debug_bounds("shell-status-bar")
            .expect("status bar should render");

        assert!(
            sidebar.origin.x < main_content.origin.x,
            "sidebar should be to the left of main content"
        );
        assert!(
            status_bar.origin.y >= sidebar.origin.y + sidebar.size.height,
            "status bar should be docked below the sidebar/main-content row"
        );
        assert!(
            status_bar.size.height > px(0.0),
            "status bar should have a non-zero height, not collapse to nothing"
        );
    }
}
