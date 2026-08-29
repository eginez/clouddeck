//! Port of `SidebarView.swift` (the Swift CloudDeck oracle's navigation
//! sidebar): app header, a "Workspace" section listing the app's nav
//! destination(s), and an "Active Context" section summarizing the
//! currently selected profile/region/provider.
//!
//! The oracle only ever has a single nav destination ("EC2 Fleet") and
//! renders it with a neutral `SurgicalTheme.surface` background (it has no
//! selected/unselected concept of its own, since there's nothing else to
//! select). This port keeps the oracle's default (unselected) appearance
//! for that card, and layers the theme bead's `rowFill(selected:...)`
//! highlight on top when a nav item's `selected` flag is set — this is an
//! intentional, forward-looking extension (per this bead's acceptance
//! criteria) so the highlight styling exists once a future bead adds real
//! multi-destination navigation. Wiring selection to actually switch
//! main-content views, and threading through live `AppState` values for
//! the active context rows, belongs to whichever bead composes the shell
//! with real content. See
//! `~/src/cloudDeck/main/Sources/CloudDeck/Views/Sidebar/SidebarView.swift`
//! for the oracle this was ported from.

use gpui::{
    BoxShadow, Context, IntoElement, ParentElement, Render, Styled, Window, div, point, prelude::*,
    px,
};

use crate::theme::{SurgicalTheme, SurgicalTypography};

/// A single sidebar navigation destination, mirroring the "Workspace"
/// section's `HStack` row in the oracle (icon + label + subtitle).
pub struct NavItem {
    pub label: &'static str,
    pub subtitle: &'static str,
    pub selected: bool,
}

/// Static nav destination list. The oracle only ever renders one
/// ("EC2 Fleet"); additional destinations are out of scope for this bead.
pub fn workspace_nav_items() -> Vec<NavItem> {
    vec![NavItem {
        label: "EC2 Fleet",
        subtitle: "Inspect instances and metadata",
        selected: true,
    }]
}

/// A single "Active Context" summary row, mirroring `contextRow(label:
/// value:systemImage:)` in the oracle.
pub struct ContextRow {
    pub label: &'static str,
    pub value: &'static str,
}

/// Static active-context rows. Real values come from `AppState` in the
/// oracle; wiring live values through is out of scope for this bead.
pub fn active_context_rows() -> Vec<ContextRow> {
    vec![
        ContextRow {
            label: "Profile",
            value: "No profile selected",
        },
        ContextRow {
            label: "Region",
            value: "Fleet (curated regions)",
        },
        ContextRow {
            label: "Provider",
            value: "AWS",
        },
    ]
}

/// Port of `SurgicalTheme.rowFill(selected:hovered:emphasized:)` as used
/// by a nav row: selected rows get the highlighted fill, unselected rows
/// fall back to the oracle's default `SurgicalTheme.surface` card
/// background.
pub fn nav_item_background(selected: bool) -> gpui::Hsla {
    SurgicalTheme::row_fill(selected, false, false).unwrap_or_else(SurgicalTheme::surface)
}

/// `SurgicalTheme.shadow` applied at the oracle's `radius: 2, x: 0, y: 1`
/// for the workspace/active-context cards.
fn card_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: SurgicalTheme::shadow(),
        offset: point(px(0.0), px(1.0)),
        blur_radius: px(2.0),
        spread_radius: px(0.0),
    }]
}

/// A small square glyph placeholder standing in for the oracle's SF Symbol
/// icons (`server.rack`, `person.crop.circle`, `location`, `cloud.fill`),
/// which have no gpui/cross-platform equivalent here.
fn icon_placeholder(color: gpui::Hsla) -> impl IntoElement {
    div()
        .w(px(8.0))
        .h(px(8.0))
        .mt(px(4.0))
        .rounded(px(2.0))
        .bg(color)
}

/// `SidebarView` port: header, "Workspace" nav section, "Active Context"
/// section.
pub struct Sidebar;

fn section_header(title: &'static str) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .justify_between()
        .text_size(px(SurgicalTypography::LABEL_SIZE))
        .text_color(SurgicalTheme::text_primary())
        .child(title.to_uppercase())
}

impl Render for Sidebar {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .debug_selector(|| "sidebar-root".into())
            .flex()
            .flex_col()
            .gap(px(22.0))
            .px(px(16.0))
            .pt(px(18.0))
            .pb(px(14.0))
            .size_full()
            .bg(SurgicalTheme::chrome())
            .child(
                div()
                    .debug_selector(|| "sidebar-header".into())
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(20.0))
                            .text_color(SurgicalTheme::text_primary())
                            .child("CloudDeck"),
                    )
                    .child(
                        div()
                            .text_size(px(SurgicalTypography::METADATA_SIZE))
                            .text_color(SurgicalTheme::text_secondary())
                            .child("EC2 fleet workbench"),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "sidebar-section-workspace".into())
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(section_header("Workspace"))
                    .children(
                        workspace_nav_items()
                            .into_iter()
                            .enumerate()
                            .map(|(i, item)| {
                                div()
                                    .debug_selector(move || format!("sidebar-nav-item-{i}").into())
                                    .flex()
                                    .flex_row()
                                    .gap(px(12.0))
                                    .px(px(14.0))
                                    .py(px(12.0))
                                    .rounded(px(10.0))
                                    .bg(nav_item_background(item.selected))
                                    .border_1()
                                    .border_color({
                                        let mut c = SurgicalTheme::outline_variant();
                                        c.a = 0.35;
                                        c
                                    })
                                    .shadow(card_shadow())
                                    .child(icon_placeholder(SurgicalTheme::primary()))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(4.0))
                                            .child(
                                                div()
                                                    .text_size(px(
                                                        SurgicalTypography::BODY_EMPHASIS_SIZE,
                                                    ))
                                                    .text_color(SurgicalTheme::text_primary())
                                                    .child(item.label),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(SurgicalTypography::CAPTION_SIZE))
                                                    .text_color(SurgicalTheme::text_secondary())
                                                    .child(item.subtitle),
                                            ),
                                    )
                            }),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "sidebar-section-context".into())
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(section_header("Active Context"))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .px(px(14.0))
                            .py(px(12.0))
                            .rounded(px(10.0))
                            .bg(SurgicalTheme::surface())
                            .border_1()
                            .border_color({
                                let mut c = SurgicalTheme::outline_variant();
                                c.a = 0.35;
                                c
                            })
                            .shadow(card_shadow())
                            .children(active_context_rows().into_iter().enumerate().flat_map(
                                |(i, row)| {
                                    let is_last = i + 1 == active_context_rows().len();
                                    let row_el = div()
                                        .debug_selector(move || {
                                            format!("sidebar-context-row-{i}").into()
                                        })
                                        .flex()
                                        .flex_row()
                                        .gap(px(12.0))
                                        .py(px(8.0))
                                        .child(icon_placeholder(SurgicalTheme::text_secondary()))
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap(px(3.0))
                                                .child(
                                                    div()
                                                        .text_size(px(
                                                            SurgicalTypography::LABEL_SIZE,
                                                        ))
                                                        .text_color(SurgicalTheme::text_secondary())
                                                        .child(row.label),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(px(
                                                            SurgicalTypography::BODY_EMPHASIS_SIZE,
                                                        ))
                                                        .text_color(SurgicalTheme::text_primary())
                                                        .child(row.value),
                                                ),
                                        )
                                        .into_any_element();

                                    if is_last {
                                        vec![row_el]
                                    } else {
                                        let divider = div()
                                            .h(px(1.0))
                                            .w_full()
                                            .bg({
                                                let mut c = SurgicalTheme::outline_variant();
                                                c.a = 0.25;
                                                c
                                            })
                                            .into_any_element();
                                        vec![row_el, divider]
                                    }
                                },
                            )),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[test]
    fn nav_item_background_is_highlighted_only_when_selected() {
        assert_ne!(
            nav_item_background(true),
            nav_item_background(false),
            "selected nav item should have a visibly different fill than unselected"
        );
        assert_eq!(
            nav_item_background(false),
            SurgicalTheme::surface(),
            "unselected nav item should fall back to the oracle's default surface fill"
        );
    }

    #[test]
    fn workspace_nav_items_include_a_selected_destination() {
        let items = workspace_nav_items();
        assert!(
            !items.is_empty(),
            "sidebar should list at least one nav destination"
        );
        assert!(
            items.iter().any(|i| i.selected),
            "at least one nav destination should be selected/highlighted"
        );
    }

    #[gpui::test]
    fn mounts_without_a_display(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| Sidebar);

        assert!(
            cx.debug_bounds("sidebar-root").is_some(),
            "sidebar root should render"
        );
    }

    #[gpui::test]
    fn renders_header_and_both_sections(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| Sidebar);

        assert!(
            cx.debug_bounds("sidebar-header").is_some(),
            "header should render"
        );
        assert!(
            cx.debug_bounds("sidebar-section-workspace").is_some(),
            "workspace section should render"
        );
        assert!(
            cx.debug_bounds("sidebar-section-context").is_some(),
            "active context section should render"
        );
    }

    #[gpui::test]
    fn renders_the_selected_nav_item_and_context_rows(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| Sidebar);

        assert!(
            cx.debug_bounds("sidebar-nav-item-0").is_some(),
            "the workspace nav item should render"
        );
        const CONTEXT_ROW_SELECTORS: [&str; 3] = [
            "sidebar-context-row-0",
            "sidebar-context-row-1",
            "sidebar-context-row-2",
        ];
        assert_eq!(active_context_rows().len(), CONTEXT_ROW_SELECTORS.len());
        for selector in CONTEXT_ROW_SELECTORS {
            assert!(
                cx.debug_bounds(selector).is_some(),
                "{selector} should render"
            );
        }
    }

    #[gpui::test]
    fn sections_are_stacked_top_to_bottom(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| Sidebar);

        let header = cx.debug_bounds("sidebar-header").expect("header renders");
        let workspace = cx
            .debug_bounds("sidebar-section-workspace")
            .expect("workspace section renders");
        let context = cx
            .debug_bounds("sidebar-section-context")
            .expect("context section renders");

        assert!(header.origin.y < workspace.origin.y);
        assert!(workspace.origin.y < context.origin.y);
    }
}
