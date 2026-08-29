//! Port of `InstanceDetailView.swift` (the Swift CloudDeck oracle's detail
//! panel for a selected instance) to gpui.
//!
//! Driven entirely by an [`InstanceDetailViewModel`]: a header (display
//! name + status + instance ID), an "Instance" metadata section, a
//! "Network" metadata section, a "Security Groups" list section, and a
//! "Tags" section. See
//! `~/src/cloudDeck/main/Sources/CloudDeck/Views/EC2/InstanceDetailView.swift`
//! for the oracle this was ported from.
//!
//! The oracle's `StatusBadge` (a separate shared view, not yet ported) is
//! stood in for here with an inline dot + label using
//! `SurgicalTheme::state_color`, the same pattern `status_bar.rs` uses for
//! its own state indicator.

use gpui::{Context, IntoElement, ParentElement, Render, Styled, Window, div, prelude::*, px};

use crate::theme::{SurgicalTheme, SurgicalTypography, radius};
use crate::view_models::InstanceDetailViewModel;

/// Port of `InstanceDetailView`. Owns the view-model for the currently
/// selected instance and renders its state; there is no view-local
/// mutable state of its own (the oracle's `InstanceDetailView` is a plain,
/// stateless `View` over its view-model).
pub struct InstanceDetailView {
    pub view_model: InstanceDetailViewModel,
}

impl InstanceDetailView {
    pub fn new(view_model: InstanceDetailViewModel) -> Self {
        Self { view_model }
    }
}

/// Port of `SurgicalSectionHeader`.
fn section_header(title: &'static str) -> impl IntoElement {
    div()
        .text_size(px(SurgicalTypography::LABEL_SIZE))
        .text_color(SurgicalTheme::text_secondary())
        .child(title.to_uppercase())
}

/// A single label/value row inside a `metadataSection`, mirroring the
/// oracle's `HStack` row builder (label column fixed at 96pt, mono font
/// applied only to the "Instance ID" row).
struct MetadataRow {
    selector: &'static str,
    label: &'static str,
    value: String,
    mono: bool,
}

/// Port of `metadataSection(title:rows:)`: a titled panel containing a
/// stack of label/value rows.
fn metadata_section(
    selector: &'static str,
    title: &'static str,
    rows: Vec<MetadataRow>,
) -> impl IntoElement {
    let mut rows_col = div().flex().flex_col();
    for row in rows {
        rows_col = rows_col.child(
            div()
                .debug_selector(move || row.selector.into())
                .flex()
                .flex_row()
                .gap(px(16.0))
                .py(px(10.0))
                .child(
                    div()
                        .w(px(96.0))
                        .text_size(px(SurgicalTypography::LABEL_SIZE))
                        .text_color(SurgicalTheme::text_secondary())
                        .child(row.label.to_uppercase()),
                )
                .child(
                    div()
                        .flex_1()
                        .text_size(px(if row.mono {
                            SurgicalTypography::MONO_SIZE
                        } else {
                            SurgicalTypography::BODY_SIZE
                        }))
                        .text_color(SurgicalTheme::text_primary())
                        .child(row.value),
                ),
        );
    }

    div()
        .debug_selector(move || selector.into())
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(section_header(title))
        .child(
            div()
                .flex()
                .flex_col()
                .p(px(14.0))
                .rounded(px(radius::LARGE))
                .bg(SurgicalTheme::surface_low())
                .child(rows_col),
        )
}

/// Port of `listSection(title:values:)`: a titled panel containing a
/// stack of plain value chips (used for security group descriptions).
fn list_section(
    selector: &'static str,
    title: &'static str,
    values: Vec<String>,
) -> impl IntoElement {
    let mut list_col = div().flex().flex_col().gap(px(6.0));
    for (i, value) in values.into_iter().enumerate() {
        let is_none = value == "None";
        list_col = list_col.child(
            div()
                .debug_selector(move || format!("{selector}-item-{i}"))
                .px(px(12.0))
                .py(px(9.0))
                .rounded(px(radius::STANDARD))
                .bg(SurgicalTheme::surface_container())
                .text_size(px(SurgicalTypography::BODY_SIZE))
                .text_color(if is_none {
                    SurgicalTheme::text_secondary()
                } else {
                    SurgicalTheme::text_primary()
                })
                .child(value),
        );
    }

    div()
        .debug_selector(move || selector.into())
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(section_header(title))
        .child(
            div()
                .flex()
                .flex_col()
                .p(px(14.0))
                .rounded(px(radius::LARGE))
                .bg(SurgicalTheme::surface_low())
                .child(list_col),
        )
}

/// Port of `tagsSection`: same panel shell as `metadataSection`/
/// `listSection`, but with a key/value row per tag (or a single "None"
/// chip when there are no tags), mirroring the oracle's tag row layout
/// (120pt key column) rather than the label/value metadata row layout.
fn tags_section(tags: Vec<clouddeck_core::EC2InstanceTag>) -> impl IntoElement {
    let mut list_col = div().flex().flex_col().gap(px(6.0));

    if tags.is_empty() {
        list_col = list_col.child(
            div()
                .debug_selector(|| "instance-detail-tags-empty".into())
                .px(px(12.0))
                .py(px(10.0))
                .rounded(px(radius::STANDARD))
                .bg(SurgicalTheme::surface_container())
                .text_size(px(SurgicalTypography::BODY_SIZE))
                .text_color(SurgicalTheme::text_secondary())
                .child("None"),
        );
    } else {
        for (i, tag) in tags.into_iter().enumerate() {
            list_col = list_col.child(
                div()
                    .debug_selector(move || format!("instance-detail-tag-{i}"))
                    .flex()
                    .flex_row()
                    .gap(px(16.0))
                    .px(px(12.0))
                    .py(px(10.0))
                    .rounded(px(radius::STANDARD))
                    .bg(SurgicalTheme::surface_container())
                    .child(
                        div()
                            .w(px(120.0))
                            .text_size(px(SurgicalTypography::METADATA_SIZE))
                            .text_color(SurgicalTheme::text_primary())
                            .child(tag.key),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(SurgicalTypography::BODY_SIZE))
                            .text_color(SurgicalTheme::text_secondary())
                            .child(tag.value),
                    ),
            );
        }
    }

    div()
        .debug_selector(|| "instance-detail-section-tags".into())
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(section_header("Tags"))
        .child(
            div()
                .flex()
                .flex_col()
                .p(px(14.0))
                .rounded(px(radius::LARGE))
                .bg(SurgicalTheme::surface_low())
                .child(list_col),
        )
}

impl Render for InstanceDetailView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let vm = &self.view_model;
        let instance = vm.instance();
        let display_name = vm.display_name();
        let instance_id = instance.instance_id.clone();

        let instance_rows = vec![
            MetadataRow {
                selector: "instance-detail-row-name",
                label: "Name",
                value: display_name.clone(),
                mono: false,
            },
            MetadataRow {
                selector: "instance-detail-row-instance-id",
                label: "Instance ID",
                value: instance_id.clone(),
                mono: true,
            },
            MetadataRow {
                selector: "instance-detail-row-region",
                label: "Region",
                value: InstanceDetailViewModel::value_or_dash(instance.region.as_deref()),
                mono: false,
            },
            MetadataRow {
                selector: "instance-detail-row-type",
                label: "Type",
                value: InstanceDetailViewModel::value_or_dash(instance.instance_type.as_deref()),
                mono: false,
            },
            MetadataRow {
                selector: "instance-detail-row-launch-time",
                label: "Launch Time",
                value: vm.launch_time_description(),
                mono: false,
            },
        ];

        let network_rows = vec![
            MetadataRow {
                selector: "instance-detail-row-public-ip",
                label: "Public IP",
                value: InstanceDetailViewModel::value_or_dash(
                    instance.public_ip_address.as_deref(),
                ),
                mono: false,
            },
            MetadataRow {
                selector: "instance-detail-row-private-ip",
                label: "Private IP",
                value: InstanceDetailViewModel::value_or_dash(
                    instance.private_ip_address.as_deref(),
                ),
                mono: false,
            },
            MetadataRow {
                selector: "instance-detail-row-vpc",
                label: "VPC",
                value: InstanceDetailViewModel::value_or_dash(instance.vpc_id.as_deref()),
                mono: false,
            },
            MetadataRow {
                selector: "instance-detail-row-subnet",
                label: "Subnet",
                value: InstanceDetailViewModel::value_or_dash(instance.subnet_id.as_deref()),
                mono: false,
            },
        ];

        let security_group_descriptions = vm.security_group_descriptions();
        let sorted_tags = vm.sorted_tags();
        let state = instance.state.as_deref();
        let state_color = SurgicalTheme::state_color(state);
        let state_label = state.unwrap_or("unknown").to_string();

        div()
            .debug_selector(|| "instance-detail-root".into())
            .flex()
            .flex_col()
            .gap(px(18.0))
            .p(px(24.0))
            .bg(SurgicalTheme::surface())
            .child(
                // header
                div()
                    .debug_selector(|| "instance-detail-header".into())
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(section_header("Inspector"))
                    .child(
                        div()
                            .debug_selector(|| "instance-detail-title".into())
                            .text_size(px(SurgicalTypography::VIEW_TITLE_SIZE))
                            .text_color(SurgicalTheme::text_primary())
                            .child(display_name),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(14.0))
                            .child(
                                div()
                                    .debug_selector(|| "instance-detail-badge".into())
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .w(px(7.0))
                                            .h(px(7.0))
                                            .rounded(px(3.5))
                                            .bg(state_color),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(SurgicalTypography::CAPTION_SIZE))
                                            .text_color(state_color)
                                            .child(state_label),
                                    ),
                            )
                            .child(
                                div()
                                    .debug_selector(|| "instance-detail-instance-id".into())
                                    .text_size(px(SurgicalTypography::MONO_SIZE))
                                    .text_color(SurgicalTheme::text_secondary())
                                    .child(instance_id),
                            ),
                    ),
            )
            .child(metadata_section(
                "instance-detail-section-instance",
                "Instance",
                instance_rows,
            ))
            .child(metadata_section(
                "instance-detail-section-network",
                "Network",
                network_rows,
            ))
            .child(list_section(
                "instance-detail-section-security-groups",
                "Security Groups",
                security_group_descriptions,
            ))
            .child(tags_section(sorted_tags))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clouddeck_core::{EC2Instance, EC2InstanceTag, EC2SecurityGroup};
    use gpui::TestAppContext;
    use std::time::{Duration, UNIX_EPOCH};

    fn populated_instance() -> EC2Instance {
        EC2Instance {
            instance_id: "i-1234567890abcdef0".to_string(),
            region: Some("us-east-1".to_string()),
            instance_type: Some("t3.medium".to_string()),
            state: Some("running".to_string()),
            launch_time: Some(UNIX_EPOCH + Duration::from_secs(1776688440)),
            public_ip_address: Some("203.0.113.10".to_string()),
            private_ip_address: Some("10.0.0.5".to_string()),
            vpc_id: Some("vpc-abc123".to_string()),
            subnet_id: Some("subnet-def456".to_string()),
            security_groups: vec![
                EC2SecurityGroup {
                    group_id: Some("sg-1".to_string()),
                    group_name: Some("web".to_string()),
                },
                EC2SecurityGroup {
                    group_id: Some("sg-2".to_string()),
                    group_name: None,
                },
            ],
            tags: vec![
                EC2InstanceTag {
                    key: "Name".to_string(),
                    value: "api-server".to_string(),
                },
                EC2InstanceTag {
                    key: "env".to_string(),
                    value: "prod".to_string(),
                },
            ],
        }
    }

    #[gpui::test]
    fn mounts_without_a_display(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| {
            InstanceDetailView::new(InstanceDetailViewModel::new(populated_instance()))
        });

        assert!(
            cx.debug_bounds("instance-detail-root").is_some(),
            "instance detail root should render"
        );
    }

    #[gpui::test]
    fn renders_header_and_all_sections(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| {
            InstanceDetailView::new(InstanceDetailViewModel::new(populated_instance()))
        });

        assert!(cx.debug_bounds("instance-detail-header").is_some());
        assert!(cx.debug_bounds("instance-detail-title").is_some());
        assert!(cx.debug_bounds("instance-detail-badge").is_some());
        assert!(cx.debug_bounds("instance-detail-instance-id").is_some());
        assert!(
            cx.debug_bounds("instance-detail-section-instance")
                .is_some()
        );
        assert!(cx.debug_bounds("instance-detail-section-network").is_some());
        assert!(
            cx.debug_bounds("instance-detail-section-security-groups")
                .is_some()
        );
        assert!(cx.debug_bounds("instance-detail-section-tags").is_some());
    }

    #[gpui::test]
    fn renders_all_instance_and_network_fields(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| {
            InstanceDetailView::new(InstanceDetailViewModel::new(populated_instance()))
        });

        const INSTANCE_ROW_SELECTORS: [&str; 5] = [
            "instance-detail-row-name",
            "instance-detail-row-instance-id",
            "instance-detail-row-region",
            "instance-detail-row-type",
            "instance-detail-row-launch-time",
        ];
        const NETWORK_ROW_SELECTORS: [&str; 4] = [
            "instance-detail-row-public-ip",
            "instance-detail-row-private-ip",
            "instance-detail-row-vpc",
            "instance-detail-row-subnet",
        ];

        for selector in INSTANCE_ROW_SELECTORS
            .into_iter()
            .chain(NETWORK_ROW_SELECTORS)
        {
            assert!(
                cx.debug_bounds(selector).is_some(),
                "{selector} should render"
            );
        }
    }

    #[gpui::test]
    fn renders_a_row_per_security_group_description(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| {
            InstanceDetailView::new(InstanceDetailViewModel::new(populated_instance()))
        });

        assert!(
            cx.debug_bounds("instance-detail-section-security-groups-item-0")
                .is_some(),
            "first security group description should render"
        );
        assert!(
            cx.debug_bounds("instance-detail-section-security-groups-item-1")
                .is_some(),
            "second security group description should render"
        );
    }

    #[gpui::test]
    fn renders_a_row_per_tag_in_sorted_order(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| {
            InstanceDetailView::new(InstanceDetailViewModel::new(populated_instance()))
        });

        // `sorted_tags` orders case-insensitively by key: "env" before
        // "Name".
        assert!(cx.debug_bounds("instance-detail-tag-0").is_some());
        assert!(cx.debug_bounds("instance-detail-tag-1").is_some());
    }

    #[gpui::test]
    fn renders_empty_state_when_there_are_no_tags(cx: &mut TestAppContext) {
        let mut instance = populated_instance();
        instance.tags = vec![];
        let (_view, cx) = cx.add_window_view(|_, _| {
            InstanceDetailView::new(InstanceDetailViewModel::new(instance))
        });

        assert!(cx.debug_bounds("instance-detail-tags-empty").is_some());
        assert!(cx.debug_bounds("instance-detail-tag-0").is_none());
    }

    #[gpui::test]
    fn sections_are_stacked_top_to_bottom(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| {
            InstanceDetailView::new(InstanceDetailViewModel::new(populated_instance()))
        });

        let header = cx
            .debug_bounds("instance-detail-header")
            .expect("header renders");
        let instance_section = cx
            .debug_bounds("instance-detail-section-instance")
            .expect("instance section renders");
        let network_section = cx
            .debug_bounds("instance-detail-section-network")
            .expect("network section renders");
        let security_groups_section = cx
            .debug_bounds("instance-detail-section-security-groups")
            .expect("security groups section renders");
        let tags_section = cx
            .debug_bounds("instance-detail-section-tags")
            .expect("tags section renders");

        assert!(header.origin.y < instance_section.origin.y);
        assert!(instance_section.origin.y < network_section.origin.y);
        assert!(network_section.origin.y < security_groups_section.origin.y);
        assert!(security_groups_section.origin.y < tags_section.origin.y);
    }
}
