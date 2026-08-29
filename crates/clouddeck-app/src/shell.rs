//! Port of `ContentView.swift` (the Swift CloudDeck oracle's top-level app
//! shell layout): a `NavigationSplitView`-style sidebar/main-content region
//! docked above a bottom status bar.
//!
//! This is structure only — the sidebar, main content, and status bar are
//! empty placeholder panels here. Their real content lands in their own
//! beads (sidebar view, EC2 tab, `CloudContextStatusBar`); this module just
//! establishes the region layout and theme surfaces they'll render into.
//! See
//! `~/src/cloudDeck/main/Sources/CloudDeck/Views/ContentView.swift` for the
//! oracle this was ported from.

use gpui::{Context, IntoElement, ParentElement, Render, Styled, Window, div, prelude::*, px};

use crate::theme::SurgicalTheme;

/// Top-level app shell: sidebar | main content, with a status bar docked
/// at the bottom. Mirrors `ContentView`'s `VStack { NavigationSplitView {
/// sidebar } detail: { main } ; statusBar }` structure.
pub struct AppShell;

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
                        // Sidebar: `SidebarView` placeholder, `minWidth: 220,
                        // idealWidth: 248` in the oracle.
                        div()
                            .debug_selector(|| "shell-sidebar".into())
                            .flex_shrink_0()
                            .w(px(248.0))
                            .min_w(px(220.0))
                            .h_full()
                            .bg(SurgicalTheme::chrome()),
                    )
                    .child(
                        // Main content: `EC2Tab` placeholder, `minWidth: 700,
                        // minHeight: 480` in the oracle.
                        div()
                            .debug_selector(|| "shell-main-content".into())
                            .flex_1()
                            .min_w(px(700.0))
                            .min_h(px(480.0))
                            .h_full()
                            .bg(SurgicalTheme::workspace_canvas()),
                    ),
            )
            .child(
                // Bottom status bar: `CloudContextStatusBar` placeholder.
                // A nominal height keeps this region visibly proportioned
                // pending its real content (its own bead); it isn't
                // collapsed to zero.
                div()
                    .debug_selector(|| "shell-status-bar".into())
                    .flex_shrink_0()
                    .w_full()
                    .h(px(28.0))
                    .bg(SurgicalTheme::chrome_dark()),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    fn mounts_without_a_display(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| AppShell);

        assert!(
            cx.debug_bounds("app-shell").is_some(),
            "app-shell root should render"
        );
    }

    #[gpui::test]
    fn renders_all_three_regions(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| AppShell);

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
    }

    #[gpui::test]
    fn sidebar_and_main_content_are_laid_out_side_by_side(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| AppShell);

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
