//! Port of `StatusBadge.swift` (the Swift CloudDeck oracle's per-row status
//! pill): a colored dot + capitalized state label, colored via
//! `SurgicalTheme.stateColor(for:)`. See
//! `~/src/cloudDeck/main/Sources/CloudDeck/Views/EC2/StatusBadge.swift` for
//! the oracle this was ported from.

use gpui::{IntoElement, ParentElement, Styled, div, prelude::*, px};

use crate::theme::{SurgicalTheme, SurgicalTypography};

/// Port of `StatusBadge`'s `normalizedStatus.capitalized` label: `status`
/// lowercased (falling back to `"unknown"` when absent, matching the
/// oracle's `status ?? "unknown"`), with only the first character
/// capitalized. This differs from Swift's full `.capitalized` (which
/// title-cases every whitespace-delimited word) only for multi-word
/// statuses, which don't occur in EC2 instance states.
pub fn status_badge_label(status: Option<&str>) -> String {
    capitalize_first(&status.unwrap_or("unknown").to_lowercase())
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Port of `StatusBadge`'s body: a small filled circle followed by the
/// capitalized status label, both colored via
/// `SurgicalTheme.stateColor(for:)`.
pub fn status_badge(selector: &'static str, status: Option<&str>) -> impl IntoElement {
    let color = SurgicalTheme::state_color(status);
    let label = status_badge_label(status);

    div()
        .debug_selector(move || selector.into())
        .flex()
        .flex_row()
        .items_center()
        .gap(px(7.0))
        .child(div().w(px(6.0)).h(px(6.0)).rounded(px(3.0)).bg(color))
        .child(
            div()
                .text_size(px(SurgicalTypography::METADATA_SIZE))
                .text_color(color)
                .child(label),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[test]
    fn label_falls_back_to_unknown_and_capitalizes_first_letter() {
        assert_eq!(status_badge_label(Some("running")), "Running");
        assert_eq!(status_badge_label(Some("STOPPED")), "Stopped");
        assert_eq!(status_badge_label(Some("shutting-down")), "Shutting-down");
        assert_eq!(status_badge_label(None), "Unknown");
    }

    #[gpui::test]
    fn renders_for_every_known_state_and_a_missing_state(cx: &mut TestAppContext) {
        struct Harness;
        impl gpui::Render for Harness {
            fn render(
                &mut self,
                _window: &mut gpui::Window,
                _cx: &mut gpui::Context<Self>,
            ) -> impl IntoElement {
                div()
                    .flex()
                    .flex_col()
                    .child(status_badge("badge-running", Some("running")))
                    .child(status_badge("badge-stopped", Some("stopped")))
                    .child(status_badge("badge-pending", Some("pending")))
                    .child(status_badge("badge-unknown", None))
            }
        }

        let (_view, cx) = cx.add_window_view(|_, _| Harness);

        for selector in [
            "badge-running",
            "badge-stopped",
            "badge-pending",
            "badge-unknown",
        ] {
            assert!(
                cx.debug_bounds(selector).is_some(),
                "{selector} should render"
            );
        }
    }
}
