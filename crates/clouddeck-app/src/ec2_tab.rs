//! Port of `EC2Tab.swift` (the Swift CloudDeck oracle's EC2 tab
//! master-detail composition) to gpui.
//!
//! Owns the [`InstanceListView`] (itself driven by an
//! [`InstanceListViewModel`]) and switches its layout based on the
//! view-model's `selected_instance`:
//!
//! - no selection: the list alone, rendered with
//!   `show_standalone_empty_state` set so it shows its own "Select an
//!   Instance" inspector prompt (mirrors the oracle's `else` branch:
//!   `InstanceListView(viewModel:showStandaloneEmptyState: true)`).
//! - a selection: an `HSplitView`-style row with the list (width-clamped,
//!   `show_standalone_empty_state` cleared) beside a retained
//!   [`InstanceDetailView`] for the selected instance. The detail entity is
//!   replaced only when the selected ID changes, preserving its scroll state
//!   while the list re-renders.
//!
//! See
//! `~/src/cloudDeck/main/Sources/CloudDeck/Views/EC2/EC2Tab.swift` for the
//! oracle this was ported from.

use gpui::{
    Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div, prelude::*, px,
};

use crate::instance_detail::InstanceDetailView;
use crate::instance_list::InstanceListView;
use crate::theme::SurgicalTheme;
use crate::view_models::{InstanceDetailViewModel, InstanceListViewModel};

/// Port of `EC2Tab`. Owns the list view's entity and retains the detail entity
/// for the selected instance so inspector scrolling survives list updates.
pub struct EC2Tab {
    pub list: Entity<InstanceListView>,
    detail: Option<(String, Entity<InstanceDetailView>)>,
}

impl EC2Tab {
    pub fn new(view_model: InstanceListViewModel, cx: &mut Context<Self>) -> Self {
        let list = cx.new(|_| InstanceListView::new(view_model));
        cx.observe(&list, |_, _, cx| cx.notify()).detach();
        Self { list, detail: None }
    }
}

impl Render for EC2Tab {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected_instance = self.list.read(cx).view_model.selected_instance().cloned();

        if let Some(instance) = selected_instance {
            if self.list.read(cx).show_standalone_empty_state {
                self.list.update(cx, |list, _| {
                    list.show_standalone_empty_state = false;
                });
            }
            let selected_id = instance.id();
            let detail = if let Some((detail_id, detail)) = self.detail.take() {
                if detail_id == selected_id {
                    if detail.read(cx).view_model.instance() != &instance {
                        detail.update(cx, |detail, _| {
                            detail.view_model = InstanceDetailViewModel::new(instance.clone());
                        });
                    }
                    self.detail = Some((detail_id, detail.clone()));
                    detail
                } else {
                    let detail = cx.new(|_| {
                        InstanceDetailView::new(InstanceDetailViewModel::new(instance.clone()))
                    });
                    self.detail = Some((selected_id, detail.clone()));
                    detail
                }
            } else {
                let detail = cx.new(|_| {
                    InstanceDetailView::new(InstanceDetailViewModel::new(instance.clone()))
                });
                self.detail = Some((selected_id, detail.clone()));
                detail
            };

            div()
                .debug_selector(|| "ec2.tab.split".into())
                .flex()
                .flex_row()
                .size_full()
                .child(
                    div()
                        .debug_selector(|| "ec2.tab.split.list".into())
                        .flex_shrink_0()
                        .min_w(px(440.0))
                        .max_w(px(680.0))
                        .w(px(560.0))
                        .h_full()
                        .bg(SurgicalTheme::surface_low())
                        .child(self.list.clone()),
                )
                .child(
                    div()
                        .debug_selector(|| "ec2.tab.split.detail".into())
                        .flex_1()
                        .h_full()
                        .bg(SurgicalTheme::surface())
                        .child(detail),
                )
                .into_any_element()
        } else {
            self.detail = None;
            if !self.list.read(cx).show_standalone_empty_state {
                self.list.update(cx, |list, _| {
                    list.show_standalone_empty_state = true;
                });
            }

            div()
                .debug_selector(|| "ec2.tab.list-only".into())
                .size_full()
                .bg(SurgicalTheme::surface_low())
                .child(self.list.clone())
                .into_any_element()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clouddeck_core::{EC2Instance, EC2InstanceTag, MockComputeService};
    use gpui::TestAppContext;

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
            availability_zone: None,
            image_id: None,
            key_name: None,
            architecture: None,
            state_transition_reason: None,
            iam_instance_profile_arn: None,
            private_dns_name: None,
            public_dns_name: None,
        }
    }

    fn populated_view_model() -> InstanceListViewModel {
        let instances = vec![
            instance("i-1", "api-server", "running"),
            instance("i-2", "bastion", "stopped"),
        ];
        let mut vm = InstanceListViewModel::new(Box::new(MockComputeService::new(instances, None)));
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(vm.load_instances());
        vm
    }

    #[gpui::test]
    fn renders_list_only_when_nothing_is_selected(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, cx| EC2Tab::new(populated_view_model(), cx));

        assert!(cx.debug_bounds("ec2.tab.list-only").is_some());
        assert!(cx.debug_bounds("ec2.tab.split").is_none());
        assert!(
            cx.debug_bounds("ec2.instance.standalone-inspector-prompt")
                .is_some(),
            "list-only layout should show the standalone empty state prompt"
        );
    }

    #[gpui::test]
    fn renders_split_layout_when_an_instance_is_selected(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, cx| EC2Tab::new(populated_view_model(), cx));

        view.update(cx, |tab, cx| {
            tab.list.update(cx, |list, _| {
                list.view_model
                    .set_selected_instance_id(Some("us-east-1:i-1".to_string()));
            });
            cx.notify();
        });
        cx.run_until_parked();

        assert!(cx.debug_bounds("ec2.tab.split").is_some());
        assert!(!view.read_with(cx, |tab, cx| tab.list.read(cx).show_standalone_empty_state));
        // GPUI 0.2.2 Frame::clear does not clear debug_bounds. Check absence
        // in a fresh window rather than reading stale selectors after a transition.
        assert!(cx.debug_bounds("ec2.tab.split.list").is_some());
        assert!(cx.debug_bounds("ec2.tab.split.detail").is_some());
        assert!(cx.debug_bounds("instance-detail-root").is_some());
        let (_, fresh) = cx.add_window_view(|_, cx| {
            let mut vm = populated_view_model();
            vm.set_selected_instance_id(Some("us-east-1:i-1".into()));
            EC2Tab::new(vm, cx)
        });
        assert!(fresh.debug_bounds("ec2.tab.list-only").is_none());
        assert!(
            fresh
                .debug_bounds("ec2.instance.standalone-inspector-prompt")
                .is_none()
        );
    }

    #[gpui::test]
    fn split_layout_places_list_before_detail(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, cx| EC2Tab::new(populated_view_model(), cx));

        view.update(cx, |tab, cx| {
            tab.list.update(cx, |list, _| {
                list.view_model
                    .set_selected_instance_id(Some("us-east-1:i-1".to_string()));
            });
            cx.notify();
        });
        cx.run_until_parked();

        let list_bounds = cx
            .debug_bounds("ec2.tab.split.list")
            .expect("list pane renders");
        let detail_bounds = cx
            .debug_bounds("ec2.tab.split.detail")
            .expect("detail pane renders");

        assert!(list_bounds.origin.x < detail_bounds.origin.x);
    }

    #[gpui::test]
    fn clicking_a_row_opens_and_then_closes_the_inspector(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, cx| EC2Tab::new(populated_view_model(), cx));
        let row = cx
            .debug_bounds("ec2.instance.row.us-east-1:i-1")
            .expect("instance row renders");

        cx.simulate_click(row.center(), Default::default());
        cx.run_until_parked();
        assert_eq!(
            view.read_with(cx, |tab, cx| {
                tab.list
                    .read(cx)
                    .view_model
                    .selected_instance_id()
                    .map(str::to_owned)
            }),
            Some("us-east-1:i-1".to_string())
        );
        assert!(cx.debug_bounds("ec2.tab.split.detail").is_some());
        assert!(cx.debug_bounds("instance-detail-root").is_some());

        let row = cx
            .debug_bounds("ec2.instance.row.us-east-1:i-1")
            .expect("selected row remains rendered");
        cx.simulate_click(row.center(), Default::default());
        cx.run_until_parked();
        assert!(view.read_with(cx, |tab, cx| {
            tab.list
                .read(cx)
                .view_model
                .selected_instance_id()
                .is_none()
        }));
        assert!(view.read_with(cx, |tab, cx| {
            tab.list.read(cx).show_standalone_empty_state
        }));
    }
}
