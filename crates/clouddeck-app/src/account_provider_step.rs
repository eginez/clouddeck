//! Port of `AccountProviderStepView` (`AccountWizardStepViews.swift` lines
//! 4-46 in the Swift CloudDeck oracle): the first account onboarding
//! wizard step, where the user picks a cloud provider.
//!
//! This mounts into the `content` region of `account_window.rs`'s
//! `AccountWindowView` (a separate bead ported the window chrome; wiring
//! this step into that chrome's placeholder content region is left to
//! whichever composing bead threads the wizard's real step views
//! together).
//!
//! The oracle's `CloudProvider` (`App/CloudProvider.swift`) is
//! `CaseIterable` with a single case today (`.aws`), and the view's own
//! comment notes "CloudDeck is AWS-only today, but the onboarding shell is
//! prepared for future providers." This port mirrors that: [`CloudProvider`]
//! is a real enum (not a hardcoded AWS-only row) so future providers slot
//! in without reshaping this file, but only `Aws` exists today. See
//! `~/src/cloudDeck/main/Sources/CloudDeck/Views/Account/AccountWizardStepViews.swift`
//! (lines 4-46) and `~/src/cloudDeck/main/Sources/CloudDeck/App/CloudProvider.swift`
//! for the oracle this was ported from.

use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement,
    Styled, Window, div, px,
};

use crate::theme::{SurgicalTheme, SurgicalTypography, radius};

/// Port of `CloudProvider` (`App/CloudProvider.swift`): the cloud
/// providers the account wizard can be configured for. `CaseIterable` in
/// the oracle with a single case (`.aws`) today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudProvider {
    Aws,
}

impl CloudProvider {
    /// Mirrors `CloudProvider.allCases` (`CaseIterable`), in order.
    pub const ALL: [CloudProvider; 1] = [CloudProvider::Aws];

    /// Port of `CloudProvider.displayName`.
    pub fn display_name(self) -> &'static str {
        match self {
            CloudProvider::Aws => "AWS",
        }
    }

    /// Port of `CloudProvider.defaultRegion`.
    pub fn default_region(self) -> &'static str {
        match self {
            CloudProvider::Aws => "us-east-1",
        }
    }

    /// Port of the row's description text ("Use AWS CLI profiles
    /// discovered from your local configuration source.").
    fn description(self) -> &'static str {
        match self {
            CloudProvider::Aws => {
                "Use AWS CLI profiles discovered from your local configuration source."
            }
        }
    }

    /// Port of the row's `accessibilityIdentifier` (`"account.provider.row.
    /// aws"`).
    fn accessibility_identifier(self) -> &'static str {
        match self {
            CloudProvider::Aws => "aws",
        }
    }
}

impl Default for CloudProvider {
    fn default() -> Self {
        CloudProvider::Aws
    }
}

/// Port of `AccountProviderStepView`: a selectable list of cloud provider
/// rows reporting the current selection back via `selected_provider`
/// (port of the oracle's `@Binding var selectedProvider: CloudProvider`).
pub struct AccountProviderStep {
    pub selected_provider: CloudProvider,
}

impl Default for AccountProviderStep {
    fn default() -> Self {
        Self {
            selected_provider: CloudProvider::default(),
        }
    }
}

impl AccountProviderStep {
    /// Port of the provider row `Button` action: `selectedProvider = ...`.
    fn select_provider(&mut self, provider: CloudProvider) {
        self.selected_provider = provider;
    }
}

impl Render for AccountProviderStep {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected_provider = self.selected_provider;

        div()
            .debug_selector(|| "account-content-provider".into())
            .flex()
            .flex_col()
            .items_start()
            .gap(px(18.0))
            .child(
                // AccountWizardStepHeader(title: "Choose Provider",
                // subtitle: "...")
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(SurgicalTypography::VIEW_TITLE_SIZE))
                            .text_color(SurgicalTheme::text_primary())
                            .child("Choose Provider"),
                    )
                    .child(
                        div()
                            .text_size(px(SurgicalTypography::BODY_SIZE))
                            .text_color(SurgicalTheme::text_secondary())
                            .child(
                                "CloudDeck is AWS-only today, but the onboarding shell is prepared for future providers.",
                            ),
                    ),
            )
            .children(CloudProvider::ALL.into_iter().map(|provider| {
                let is_selected = provider == selected_provider;
                let selector_id = format!(
                    "account-provider-row-{}",
                    provider.accessibility_identifier()
                );
                let debug_id = selector_id.clone();
                let icon_color = if is_selected {
                    SurgicalTheme::primary()
                } else {
                    SurgicalTheme::inactive()
                };

                div()
                    .id(gpui::SharedString::from(selector_id))
                    .debug_selector(move || debug_id.clone().into())
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(14.0))
                    .w_full()
                    .p(px(18.0))
                    .rounded(px(radius::LARGE))
                    .bg(SurgicalTheme::surface_low())
                    .child(
                        // Image(systemName: "cloud.fill"): no
                        // gpui/cross-platform SF Symbol equivalent, so
                        // (mirroring account_window.rs's
                        // `icon_placeholder`) this is a small filled square
                        // standing in for the glyph.
                        div()
                            .w(px(8.0))
                            .h(px(8.0))
                            .rounded(px(2.0))
                            .bg(SurgicalTheme::primary()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_size(px(SurgicalTypography::SECTION_TITLE_SIZE))
                                    .text_color(SurgicalTheme::text_primary())
                                    .child(provider.display_name()),
                            )
                            .child(
                                div()
                                    .text_size(px(SurgicalTypography::BODY_SIZE))
                                    .text_color(SurgicalTheme::text_secondary())
                                    .child(provider.description()),
                            ),
                    )
                    .child(
                        // Image(systemName: selectedProvider == .aws ?
                        // "checkmark.circle.fill" : "circle"): filled dot
                        // when selected, hollow-colored dot otherwise (see
                        // `icon_placeholder` convention above).
                        div()
                            .w(px(8.0))
                            .h(px(8.0))
                            .rounded(px(4.0))
                            .bg(icon_color),
                    )
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.select_provider(provider);
                        cx.notify();
                    }))
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[test]
    fn default_provider_matches_oracle_default() {
        assert_eq!(CloudProvider::default(), CloudProvider::Aws);
        assert_eq!(AccountProviderStep::default().selected_provider, CloudProvider::Aws);
    }

    #[test]
    fn display_name_and_default_region_match_oracle() {
        assert_eq!(CloudProvider::Aws.display_name(), "AWS");
        assert_eq!(CloudProvider::Aws.default_region(), "us-east-1");
    }

    #[test]
    fn select_provider_updates_selection() {
        let mut step = AccountProviderStep::default();
        step.select_provider(CloudProvider::Aws);
        assert_eq!(step.selected_provider, CloudProvider::Aws);
    }

    #[gpui::test]
    fn mounts_without_a_display(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| AccountProviderStep::default());

        assert!(
            cx.debug_bounds("account-content-provider").is_some(),
            "provider step should render"
        );
        assert!(cx.debug_bounds("account-provider-row-aws").is_some());
    }

    /// Mirrors `account_window.rs`'s click-simulation helper.
    fn click_selector(cx: &mut gpui::VisualTestContext, selector: &'static str) {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} should render before it can be clicked"));
        let center = bounds.center();
        cx.simulate_click(center, Default::default());
    }

    #[gpui::test]
    fn clicking_the_aws_row_reports_it_as_selected(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| AccountProviderStep::default());

        click_selector(cx, "account-provider-row-aws");

        view.update(cx, |step, _cx| {
            assert_eq!(step.selected_provider, CloudProvider::Aws);
        });
    }
}
