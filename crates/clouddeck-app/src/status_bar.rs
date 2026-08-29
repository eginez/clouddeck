//! Port of `CloudContextStatusBar.swift` (the Swift CloudDeck oracle's
//! bottom bar showing the active cloud provider/profile/region) to gpui.
//!
//! This bead ports layout and static/mock context data only. The
//! `CloudContextQuickPicker` popover that the profile/region chips launch
//! in the oracle is a separate bead; here the chips are wired to a click
//! target that toggles `active_quick_picker` state so that bead has a
//! hook point to attach to, without rendering a popover itself.
//! See
//! `~/src/cloudDeck/main/Sources/CloudDeck/Views/Shared/CloudContextStatusBar.swift`
//! for the oracle this was ported from.

use gpui::{
    BoxShadow, Context, InteractiveElement, IntoElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, Window, div, point, px,
};

use crate::theme::{SurgicalTheme, SurgicalTypography, radius};

/// `AppState.fleetRegionIdentifier` in the Swift oracle: the sentinel
/// region value meaning "curated fleet view" rather than a real AWS
/// region.
pub const FLEET_REGION_IDENTIFIER: &str = "fleet";

/// Port of the oracle's inline `switch appState.contextStatusState`
/// (an `AppState`-owned enum, not defined in this file, but consumed by
/// it for `statusIndicatorColor`/`statusText`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextStatusState {
    Ready,
    Reloading,
    SetupRequired,
    Error(String),
}

/// Port of `CloudContextQuickPickerKind`: which chip's quick-picker (a
/// separate bead) is currently toggled open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudContextQuickPickerKind {
    Profile,
    Region,
}

/// Port of `CloudContextStatusBar`. Context data (`provider`, `profile`,
/// `region`, `status`) is static/mock for this bead; live wiring to
/// `AppState` happens in a view-model bead.
pub struct CloudContextStatusBar {
    pub provider: String,
    pub profile: Option<String>,
    pub region: String,
    pub status: ContextStatusState,
    /// Hook point for the quick-picker bead: which chip's picker is
    /// toggled open. Toggled by clicking the profile/region chips, but no
    /// popover is rendered from it here.
    pub active_quick_picker: Option<CloudContextQuickPickerKind>,
}

impl Default for CloudContextStatusBar {
    fn default() -> Self {
        Self {
            provider: "AWS".into(),
            profile: None,
            region: String::new(),
            status: ContextStatusState::Ready,
            active_quick_picker: None,
        }
    }
}

impl CloudContextStatusBar {
    /// Port of `profileChip`'s title fallback: `appState.selectedProfileName
    /// ?? "Profile"`.
    fn profile_chip_title(&self) -> String {
        self.profile.clone().unwrap_or_else(|| "Profile".into())
    }

    /// Port of `regionChipTitle`.
    fn region_chip_title(&self) -> String {
        if self.region.is_empty() {
            "Region".into()
        } else if self.region == FLEET_REGION_IDENTIFIER {
            "Fleet".into()
        } else {
            self.region.clone()
        }
    }

    /// Port of `statusIndicatorColor`.
    fn status_indicator_color(&self) -> gpui::Hsla {
        match self.status {
            ContextStatusState::Ready => SurgicalTheme::text_secondary(),
            ContextStatusState::Reloading => SurgicalTheme::warning(),
            ContextStatusState::SetupRequired => SurgicalTheme::warning(),
            ContextStatusState::Error(_) => SurgicalTheme::error(),
        }
    }

    /// Port of `statusText`.
    fn status_text(&self) -> String {
        match &self.status {
            ContextStatusState::Ready => "Ready".into(),
            ContextStatusState::Reloading => "Refreshing...".into(),
            ContextStatusState::SetupRequired => "Setup Required".into(),
            ContextStatusState::Error(message) => message.clone(),
        }
    }

    /// Port of `toggleQuickPicker(_:)`.
    fn toggle_quick_picker(&mut self, kind: CloudContextQuickPickerKind) {
        self.active_quick_picker = if self.active_quick_picker == Some(kind) {
            None
        } else {
            Some(kind)
        };
    }
}

/// Port of `actionChip`'s `.shadow(color: SurgicalTheme.shadow, radius: 2,
/// x: 0, y: 1)`. gpui has no SF Symbols equivalent for the chip icons, so
/// callers substitute a short text glyph for `systemImage`.
fn chip_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: SurgicalTheme::shadow(),
        offset: point(px(0.0), px(1.0)),
        blur_radius: px(2.0),
        spread_radius: px(0.0),
    }]
}

impl Render for CloudContextStatusBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let provider_label = format!("\u{2601} {}", self.provider); // cloud glyph stand-in for systemImage: "cloud"
        let profile_title = self.profile_chip_title();
        let region_title = self.region_chip_title();
        let status_color = self.status_indicator_color();
        let status_text = self.status_text();

        let chip_border_color = {
            let mut c = SurgicalTheme::outline_variant();
            c.a *= 0.45;
            c
        };

        div()
            .id("statusbar.root")
            .debug_selector(|| "statusbar.root".into())
            .flex()
            .flex_row()
            .items_center()
            .gap(px(12.0))
            .px(px(16.0))
            .py(px(10.0))
            .bg(SurgicalTheme::chrome())
            .border_t_1()
            .border_color({
                let mut c = SurgicalTheme::outline_variant();
                c.a *= 0.35;
                c
            })
            .child(
                // providerLabel
                div()
                    .id("statusbar.provider")
                    .debug_selector(|| "statusbar.provider".into())
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(SurgicalTypography::METADATA_SIZE))
                    .text_color(SurgicalTheme::text_primary())
                    .child(provider_label)
                    .child(
                        // `Circle().fill(textSecondary.opacity(0.5))
                        // .frame(width: 3, height: 3)` separator dot.
                        div()
                            .debug_selector(|| "statusbar.provider.separator".into())
                            .w(px(3.0))
                            .h(px(3.0))
                            .rounded(px(1.5))
                            .bg({
                                let mut c = SurgicalTheme::text_secondary();
                                c.a *= 0.5;
                                c
                            }),
                    ),
            )
            .child(
                // Divider() .frame(height: 16)
                div()
                    .debug_selector(|| "statusbar.divider".into())
                    .w(px(1.0))
                    .h(px(16.0))
                    .bg(SurgicalTheme::outline_variant()),
            )
            .child(
                // profileChip
                div()
                    .id("statusbar.profile")
                    .debug_selector(|| "statusbar.profile".into())
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(12.0))
                    .py(px(7.0))
                    .rounded(px(radius::STANDARD))
                    .bg(SurgicalTheme::surface_highest())
                    .border_1()
                    .border_color(chip_border_color)
                    .shadow(chip_shadow())
                    .text_size(px(SurgicalTypography::METADATA_SIZE))
                    .text_color(SurgicalTheme::text_primary())
                    .child("\u{1F464}") // stand-in for systemImage: "person.crop.circle"
                    .child(profile_title)
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.toggle_quick_picker(CloudContextQuickPickerKind::Profile);
                        cx.notify();
                    })),
            )
            .child(
                // regionChip
                div()
                    .id("statusbar.region")
                    .debug_selector(|| "statusbar.region".into())
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(12.0))
                    .py(px(7.0))
                    .rounded(px(radius::STANDARD))
                    .bg(SurgicalTheme::surface_highest())
                    .border_1()
                    .border_color(chip_border_color)
                    .shadow(chip_shadow())
                    .text_size(px(SurgicalTypography::METADATA_SIZE))
                    .text_color(SurgicalTheme::text_primary())
                    .child("\u{1F4CD}") // stand-in for systemImage: "location"
                    .child(region_title)
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.toggle_quick_picker(CloudContextQuickPickerKind::Region);
                        cx.notify();
                    })),
            )
            .child(
                // Spacer(minLength: 16)
                div()
                    .debug_selector(|| "statusbar.spacer".into())
                    .flex_1()
                    .min_w(px(16.0)),
            )
            .child(
                // statusIndicator
                div()
                    .id("statusbar.message")
                    .debug_selector(|| "statusbar.message".into())
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .debug_selector(|| "statusbar.message.dot".into())
                            .w(px(7.0))
                            .h(px(7.0))
                            .rounded(px(3.5))
                            .bg(status_color),
                    )
                    .child(
                        div()
                            .text_size(px(SurgicalTypography::CAPTION_SIZE))
                            .text_color(status_color)
                            .child(status_text),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    fn mounts_without_a_display(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| CloudContextStatusBar::default());

        assert!(
            cx.debug_bounds("statusbar.root").is_some(),
            "statusbar.root should render"
        );
    }

    #[gpui::test]
    fn renders_all_sections(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| CloudContextStatusBar {
            provider: "AWS".into(),
            profile: Some("prod-admin".into()),
            region: "us-west-2".into(),
            status: ContextStatusState::Ready,
            active_quick_picker: None,
        });

        assert!(cx.debug_bounds("statusbar.provider").is_some());
        assert!(cx.debug_bounds("statusbar.profile").is_some());
        assert!(cx.debug_bounds("statusbar.region").is_some());
        assert!(cx.debug_bounds("statusbar.message").is_some());
    }

    #[gpui::test]
    fn sections_are_laid_out_left_to_right(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| CloudContextStatusBar {
            provider: "AWS".into(),
            profile: Some("prod-admin".into()),
            region: "us-west-2".into(),
            status: ContextStatusState::Ready,
            active_quick_picker: None,
        });

        let provider = cx.debug_bounds("statusbar.provider").unwrap();
        let profile = cx.debug_bounds("statusbar.profile").unwrap();
        let region = cx.debug_bounds("statusbar.region").unwrap();
        let message = cx.debug_bounds("statusbar.message").unwrap();

        assert!(provider.origin.x < profile.origin.x);
        assert!(profile.origin.x < region.origin.x);
        assert!(
            region.origin.x < message.origin.x,
            "status indicator should be pushed to the far right by the spacer"
        );
    }

    #[test]
    fn region_chip_title_matches_oracle_fallbacks() {
        let mut bar = CloudContextStatusBar::default();
        assert_eq!(bar.region_chip_title(), "Region");

        bar.region = FLEET_REGION_IDENTIFIER.into();
        assert_eq!(bar.region_chip_title(), "Fleet");

        bar.region = "eu-central-1".into();
        assert_eq!(bar.region_chip_title(), "eu-central-1");
    }

    #[test]
    fn profile_chip_title_falls_back_when_unset() {
        let bar = CloudContextStatusBar::default();
        assert_eq!(bar.profile_chip_title(), "Profile");
    }

    #[test]
    fn status_text_and_color_match_oracle_states() {
        let mut bar = CloudContextStatusBar::default();
        assert_eq!(bar.status_text(), "Ready");

        bar.status = ContextStatusState::Reloading;
        assert_eq!(bar.status_text(), "Refreshing...");
        assert_eq!(bar.status_indicator_color(), SurgicalTheme::warning());

        bar.status = ContextStatusState::SetupRequired;
        assert_eq!(bar.status_text(), "Setup Required");
        assert_eq!(bar.status_indicator_color(), SurgicalTheme::warning());

        bar.status = ContextStatusState::Error("boom".into());
        assert_eq!(bar.status_text(), "boom");
        assert_eq!(bar.status_indicator_color(), SurgicalTheme::error());
    }

    #[test]
    fn toggle_quick_picker_flips_and_untoggles() {
        let mut bar = CloudContextStatusBar::default();
        assert_eq!(bar.active_quick_picker, None);

        bar.toggle_quick_picker(CloudContextQuickPickerKind::Profile);
        assert_eq!(
            bar.active_quick_picker,
            Some(CloudContextQuickPickerKind::Profile)
        );

        bar.toggle_quick_picker(CloudContextQuickPickerKind::Profile);
        assert_eq!(bar.active_quick_picker, None);

        bar.toggle_quick_picker(CloudContextQuickPickerKind::Region);
        assert_eq!(
            bar.active_quick_picker,
            Some(CloudContextQuickPickerKind::Region)
        );
    }
}
