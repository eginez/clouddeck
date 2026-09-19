//! Port of the remaining portion of `AccountWorkspaceSetupStepView`
//! (`AccountWizardStepViews.swift` in the Swift CloudDeck oracle) not
//! covered by `account_profile_picker.rs` (`clouddeck-rs-w6s.8`):
//! `regionRow`, `selectedProfilePanel`/`selectionHintPanel`,
//! `regionSummaryPanel`/`summaryRow`, and `errorBanner` (roughly lines
//! 307-398 and 443-488 of the oracle file).
//!
//! Region selection follows profile selection in the oracle (the region
//! hint panel reads the selected profile's configured region), so this
//! module builds on [`crate::account_profile_picker`]'s `AwsProfile` type
//! and helpers (`profile_identity`, `preferred_region_text`,
//! `region_source_label`, `region_label`, `fuzzy_match`) rather than
//! redefining them.
//!
//! Region data (`draftAvailableRegions`) is mock/static here, matching
//! this bead's scope; the oracle's `appState.preferredRegion(for:)` /
//! `appState.rememberedRegion(for:)` lookups still don't exist in this
//! port (see `account_profile_picker.rs`'s module docs), so
//! `preferred_region_fallback` / `remembered_region` remain plain fields
//! a caller sets directly. Likewise `CloudProvider`/`AppState.
//! draftSelectedProvider` don't exist yet, so `provider_display_name`
//! defaults to the literal `"AWS"` (CloudDeck is AWS-only today per
//! `AccountProviderStepView`'s own copy).
//!
//! See
//! `~/src/cloudDeck/main/Sources/CloudDeck/Views/Account/AccountWizardStepViews.swift`
//! for the oracle this was ported from.

use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement,
    Styled, Window, div, px,
};

use crate::account_profile_picker::{
    AwsProfile, fuzzy_match, preferred_region_text, profile_identity, region_label,
    region_source_label,
};
use crate::theme::{SurgicalTheme, SurgicalTypography, radius};

/// Static mock region list standing in for `appState.draftAvailableRegions`.
pub fn mock_regions() -> Vec<String> {
    [
        "us-east-1",
        "us-east-2",
        "us-west-1",
        "us-west-2",
        "eu-west-1",
        "eu-central-1",
        "ap-southeast-1",
        "ap-southeast-2",
        "ap-northeast-1",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

/// Port of `filteredRegions`: regions matching `query` (or all regions, if
/// `query` is blank) by fuzzy-matching the region code or its display
/// label.
pub fn filtered_regions(regions: &[String], query: &str) -> Vec<String> {
    let query = query.trim();
    if query.is_empty() {
        return regions.to_vec();
    }
    regions
        .iter()
        .filter(|region| fuzzy_match(query, region) || fuzzy_match(query, region_label(region)))
        .cloned()
        .collect()
}

/// Port of the region-picker, "Current Selection"/"Context Preview"
/// panels, and error banner from `AccountWorkspaceSetupStepView`. Profile
/// search/selection itself lives in
/// [`crate::account_profile_picker::AccountProfilePicker`]
/// (`clouddeck-rs-w6s.8`); this component is driven by whichever profile
/// that picker reports selected.
pub struct AccountWorkspaceSetupPanel {
    pub profiles: Vec<AwsProfile>,
    pub selected_profile_name: Option<String>,
    pub regions: Vec<String>,
    pub selected_region: String,
    pub region_search_text: String,
    /// Port of `appState.draftProfileLoadErrorMessage`. `Some` renders the
    /// `errorBanner`.
    pub error_message: Option<String>,
    /// Stand-in for `appState.preferredRegion(for:)` (see module docs).
    pub preferred_region_fallback: String,
    /// Stand-in for `appState.rememberedRegion(for:)` (see module docs).
    pub remembered_region: Option<String>,
    /// Stand-in for `appState.draftSelectedProvider.displayName` (see
    /// module docs).
    pub provider_display_name: String,
}

impl Default for AccountWorkspaceSetupPanel {
    fn default() -> Self {
        Self {
            profiles: Vec::new(),
            selected_profile_name: None,
            regions: mock_regions(),
            selected_region: "us-east-1".into(),
            region_search_text: String::new(),
            error_message: None,
            preferred_region_fallback: "us-east-1".into(),
            remembered_region: None,
            provider_display_name: "AWS".into(),
        }
    }
}

impl AccountWorkspaceSetupPanel {
    /// Port of the region search field's `text` binding.
    pub fn set_region_search_text(&mut self, text: impl Into<String>) {
        self.region_search_text = text.into();
    }

    /// Port of the `errorBanner`'s data source, `appState.
    /// draftProfileLoadErrorMessage`.
    pub fn set_error_message(&mut self, error: Option<impl Into<String>>) {
        self.error_message = error.map(Into::into);
    }

    /// Port of the `regionRow` button action: `selectedRegion = region`.
    fn select_region(&mut self, region: String) {
        self.selected_region = region;
    }

    /// Port of `filteredRegions`, applied to this panel's own state.
    pub fn filtered_regions(&self) -> Vec<String> {
        filtered_regions(&self.regions, &self.region_search_text)
    }

    /// Port of the `selectedProfile` computed property: `appState.
    /// draftAvailableProfiles.first { $0.name == selectedProfileName }`.
    pub fn selected_profile(&self) -> Option<&AwsProfile> {
        let name = self.selected_profile_name.as_deref()?;
        self.profiles.iter().find(|p| p.name == name)
    }
}

impl Render for AccountWorkspaceSetupPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let filtered = self.filtered_regions();
        let selected_region = self.selected_region.clone();
        let region_search_text = self.region_search_text.clone();
        let error_message = self.error_message.clone();
        let selected_profile = self.selected_profile().cloned();
        let selected_profile_name = self.selected_profile_name.clone();
        let provider_display_name = self.provider_display_name.clone();

        let mut root = div()
            .debug_selector(|| "account-workspace-setup-panel".into())
            .flex()
            .flex_col()
            .gap(px(14.0));

        // errorBanner
        if let Some(error) = error_message {
            root = root.child(
                div()
                    .id("account-workspace-error-state")
                    .debug_selector(|| "account-workspace-error-state".into())
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(10.0))
                    .px(px(14.0))
                    .py(px(10.0))
                    .rounded(px(radius::LARGE))
                    .bg(SurgicalTheme::surface_low())
                    .child(
                        // Port of `Image(systemName:
                        // "exclamationmark.triangle.fill")`: no gpui SF
                        // Symbol equivalent (see `regionRow`'s port for the
                        // same convention), so a small filled dot stands in
                        // for the warning glyph.
                        div()
                            .w(px(10.0))
                            .h(px(10.0))
                            .rounded(px(5.0))
                            .bg(SurgicalTheme::error()),
                    )
                    .child(
                        div()
                            .text_size(px(SurgicalTypography::BODY_SIZE))
                            .text_color(SurgicalTheme::error())
                            .child(error),
                    ),
            );
        }

        // selectedProfilePanel / selectionHintPanel
        root = root.child(match &selected_profile {
            Some(profile) => {
                let identity = profile_identity(profile);
                let preferred_region =
                    preferred_region_text(profile, &self.preferred_region_fallback);
                let source_label = region_source_label(
                    profile,
                    self.remembered_region.as_deref(),
                    &selected_region,
                );

                div()
                    .id("account-workspace-profile-selection")
                    .debug_selector(|| "account-workspace-profile-selection".into())
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .p(px(16.0))
                    .rounded(px(radius::LARGE))
                    .bg(SurgicalTheme::surface_low())
                    .child(
                        div()
                            .text_size(px(SurgicalTypography::SECTION_TITLE_SIZE))
                            .text_color(SurgicalTheme::text_secondary())
                            .child("Current Selection"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_start()
                            .justify_between()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.0))
                                    .child(
                                        div()
                                            .text_size(px(SurgicalTypography::SECTION_TITLE_SIZE))
                                            .text_color(SurgicalTheme::text_primary())
                                            .child(profile.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(SurgicalTypography::BODY_SIZE))
                                            .text_color(SurgicalTheme::text_secondary())
                                            .child(identity),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .items_end()
                                    .gap(px(4.0))
                                    .child(
                                        div()
                                            .text_size(px(SurgicalTypography::METADATA_SIZE))
                                            .text_color(SurgicalTheme::text_primary())
                                            .child(preferred_region),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(SurgicalTypography::CAPTION_SIZE))
                                            .text_color(SurgicalTheme::text_secondary())
                                            .child(source_label),
                                    ),
                            ),
                    )
            }
            None => div()
                .id("account-workspace-profile-selection-hint")
                .debug_selector(|| "account-workspace-profile-selection-hint".into())
                .flex()
                .flex_col()
                .gap(px(8.0))
                .p(px(16.0))
                .rounded(px(radius::LARGE))
                .bg(SurgicalTheme::surface_low())
                .child(
                    div()
                        .text_size(px(SurgicalTypography::SECTION_TITLE_SIZE))
                        .text_color(SurgicalTheme::text_secondary())
                        .child("Current Selection"),
                )
                .child(
                    div()
                        .text_size(px(SurgicalTypography::BODY_SIZE))
                        .text_color(SurgicalTheme::text_secondary())
                        .child(
                            "Choose a profile to see its auth type, configured region, and \
                             remembered workspace context.",
                        ),
                ),
        });

        // region search field
        root = root.child(
            div()
                .id("account-workspace-region-search")
                .debug_selector(|| "account-workspace-region-search".into())
                .px(px(11.0))
                .py(px(8.0))
                .rounded(px(radius::STANDARD))
                .bg(SurgicalTheme::surface_container())
                .text_size(px(SurgicalTypography::BODY_SIZE))
                .text_color(if region_search_text.is_empty() {
                    SurgicalTheme::text_secondary()
                } else {
                    SurgicalTheme::text_primary()
                })
                .child(if region_search_text.is_empty() {
                    "Search regions".to_string()
                } else {
                    region_search_text.clone()
                }),
        );

        // region list (regionRow x filtered)
        root = root.child(
            div()
                .id("account-workspace-region-list")
                .debug_selector(|| "account-workspace-region-list".into())
                .flex()
                .flex_col()
                .gap(px(6.0))
                .p(px(6.0))
                .rounded(px(radius::LARGE))
                .bg(SurgicalTheme::surface())
                .children(filtered.into_iter().map(|region| {
                    let is_selected = selected_region == region;
                    let selector_id = format!("account-workspace-region-row-{region}");
                    let debug_id = selector_id.clone();
                    let row_region = region.clone();
                    let label = region_label(&region).to_string();
                    // Port of `Image(systemName: isSelected ?
                    // "checkmark.circle.fill" : "circle")`: no gpui SF
                    // Symbol equivalent (see `account_profile_picker.rs`'s
                    // `profileRow` port for the same convention).
                    let icon_color = if is_selected {
                        SurgicalTheme::primary()
                    } else {
                        SurgicalTheme::inactive()
                    };

                    div()
                        .id(gpui::SharedString::from(selector_id))
                        .debug_selector(move || debug_id.clone())
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(12.0))
                        .px(px(12.0))
                        .py(px(10.0))
                        .rounded(px(radius::STANDARD))
                        .bg(SurgicalTheme::row_fill(is_selected, false, !is_selected)
                            .unwrap_or_else(SurgicalTheme::surface))
                        .child(div().w(px(8.0)).h(px(8.0)).rounded(px(4.0)).bg(icon_color))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .gap(px(4.0))
                                .child(
                                    div()
                                        .text_size(px(SurgicalTypography::BODY_EMPHASIS_SIZE))
                                        .text_color(SurgicalTheme::text_primary())
                                        .child(region.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(px(SurgicalTypography::CAPTION_SIZE))
                                        .text_color(SurgicalTheme::text_secondary())
                                        .child(label),
                                ),
                        )
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.select_region(row_region.clone());
                            cx.notify();
                        }))
                })),
        );

        // regionSummaryPanel / summaryRow (label, value)
        root.child(
            div()
                .id("account-workspace-context-preview")
                .debug_selector(|| "account-workspace-context-preview".into())
                .flex()
                .flex_col()
                .gap(px(10.0))
                .p(px(16.0))
                .rounded(px(radius::LARGE))
                .bg(SurgicalTheme::surface_low())
                .child(
                    div()
                        .text_size(px(SurgicalTypography::SECTION_TITLE_SIZE))
                        .text_color(SurgicalTheme::text_secondary())
                        .child("Context Preview"),
                )
                .child(summary_row("Provider", &provider_display_name))
                .child(summary_row(
                    "Profile",
                    selected_profile_name.as_deref().unwrap_or("None selected"),
                ))
                .child(summary_row("Region", &selected_region)),
        )
    }
}

/// Port of `summaryRow(label:value:)`. `label` is uppercased here
/// (matching the Swift call site's `Text(label.uppercased())`), so callers
/// pass a title-case label as-is.
fn summary_row(label: &str, value: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_baseline()
        .gap(px(10.0))
        .child(
            div()
                .w(px(60.0))
                .text_size(px(SurgicalTypography::LABEL_SIZE))
                .text_color(SurgicalTheme::text_secondary())
                .child(label.to_uppercase()),
        )
        .child(
            div()
                .flex_1()
                .text_size(px(SurgicalTypography::BODY_SIZE))
                .text_color(SurgicalTheme::text_primary())
                .child(value.to_string()),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account_profile_picker::mock_profiles;
    use gpui::TestAppContext;

    #[test]
    fn filtered_regions_returns_all_when_query_is_blank() {
        let regions = mock_regions();
        assert_eq!(filtered_regions(&regions, "  ").len(), regions.len());
    }

    #[test]
    fn filtered_regions_matches_code_or_label() {
        let regions = mock_regions();

        let by_code = filtered_regions(&regions, "eu-west-1");
        assert_eq!(by_code, vec!["eu-west-1".to_string()]);

        let by_label = filtered_regions(&regions, "Tokyo");
        assert_eq!(by_label, vec!["ap-northeast-1".to_string()]);

        assert!(filtered_regions(&regions, "nonexistent").is_empty());
    }

    fn panel_with_selected_profile() -> AccountWorkspaceSetupPanel {
        let profiles = mock_profiles();
        AccountWorkspaceSetupPanel {
            selected_profile_name: Some("prod-admin".into()),
            profiles,
            ..Default::default()
        }
    }

    #[test]
    fn selected_profile_looks_up_by_name() {
        let panel = panel_with_selected_profile();
        assert_eq!(
            panel.selected_profile().map(|p| p.name.as_str()),
            Some("prod-admin")
        );

        let none_selected = AccountWorkspaceSetupPanel::default();
        assert!(none_selected.selected_profile().is_none());
    }

    #[gpui::test]
    fn mounts_without_a_display(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| AccountWorkspaceSetupPanel::default());

        assert!(cx.debug_bounds("account-workspace-setup-panel").is_some());
        assert!(cx.debug_bounds("account-workspace-region-search").is_some());
        assert!(cx.debug_bounds("account-workspace-region-list").is_some());
        assert!(
            cx.debug_bounds("account-workspace-context-preview")
                .is_some()
        );
        // No profile selected -> hint panel, no error message -> no banner.
        assert!(
            cx.debug_bounds("account-workspace-profile-selection-hint")
                .is_some()
        );
        assert!(cx.debug_bounds("account-workspace-error-state").is_none());
    }

    #[gpui::test]
    fn renders_a_row_per_mock_region(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| AccountWorkspaceSetupPanel::default());

        for selector in [
            "account-workspace-region-row-us-east-1",
            "account-workspace-region-row-eu-central-1",
            "account-workspace-region-row-ap-northeast-1",
        ] {
            assert!(
                cx.debug_bounds(selector).is_some(),
                "expected a row for selector {selector}"
            );
        }
    }

    #[gpui::test]
    fn setting_region_search_text_filters_the_rendered_rows(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| AccountWorkspaceSetupPanel::default());

        view.update(cx, |panel, cx| {
            panel.set_region_search_text("Tokyo");
            cx.notify();
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("account-workspace-region-row-ap-northeast-1")
                .is_some()
        );
        assert!(
            cx.debug_bounds("account-workspace-region-row-us-east-1")
                .is_none(),
            "non-matching region should not render a row"
        );
    }

    /// Mirrors `account_profile_picker.rs`'s click-simulation helper.
    fn click_selector(cx: &mut gpui::VisualTestContext, selector: &'static str) {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} should render before it can be clicked"));
        let center = bounds.center();
        cx.simulate_click(center, Default::default());
    }

    #[gpui::test]
    fn clicking_a_region_row_reports_it_as_the_selected_region(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| AccountWorkspaceSetupPanel::default());

        click_selector(cx, "account-workspace-region-row-eu-west-1");

        view.update(cx, |panel, _cx| {
            assert_eq!(panel.selected_region, "eu-west-1");
        });
    }

    #[gpui::test]
    fn selecting_a_profile_renders_the_selected_profile_panel(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| panel_with_selected_profile());

        assert!(
            cx.debug_bounds("account-workspace-profile-selection")
                .is_some(),
            "expected the selected-profile summary panel to render"
        );
        assert!(
            cx.debug_bounds("account-workspace-profile-selection-hint")
                .is_none()
        );
    }

    #[gpui::test]
    fn setting_an_error_message_renders_the_error_banner(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| AccountWorkspaceSetupPanel::default());

        view.update(cx, |panel, cx| {
            panel.set_error_message(Some("Could not read ~/.aws/credentials"));
            cx.notify();
        });
        cx.run_until_parked();

        assert!(cx.debug_bounds("account-workspace-error-state").is_some());

        view.update(cx, |panel, cx| {
            panel.set_error_message(None::<String>);
            cx.notify();
        });
        cx.run_until_parked();

        assert!(cx.debug_bounds("account-workspace-error-state").is_none());
    }
}
