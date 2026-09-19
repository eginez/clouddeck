//! Port of the profile-search/select portion of
//! `AccountWorkspaceSetupStepView` (`AccountWizardStepViews.swift` in the
//! Swift CloudDeck oracle): the profile search field, `profileRow`,
//! `fuzzyMatch`, and the `profileIdentity`/`profileHintTag`/
//! `preferredRegionText`/`regionSourceLabel`/`regionLabel` display
//! helpers.
//!
//! This bead covers profile search + selection only. The rest of
//! `AccountWorkspaceSetupStepView` — `regionRow`, `selectedProfilePanel`,
//! `summaryRow`, the error banner, and the region picker — is a separate
//! bead (`clouddeck-rs-w6s.9`) that depends on this one, since it reuses
//! `region_label` and the profile data shape defined here.
//!
//! Profile data is mock/static here, matching this bead's scope; real
//! `~/.aws/credentials`/`config` parsing is separate fleet auth work (see
//! `~/src/cloudDeck/main/Sources/CloudDeck/Auth/AWSProfile.swift` and
//! `AWSProfileParser.swift`). The oracle also reads `appState.
//! preferredRegion(for:)` / `appState.rememberedRegion(for:)`, which don't
//! exist in this port yet; `preferred_region_text` / `region_source_label`
//! take those as plain parameters instead of an `AppState` environment.
//! See
//! `~/src/cloudDeck/main/Sources/CloudDeck/Views/Account/AccountWizardStepViews.swift`
//! for the oracle this was ported from.

use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement,
    Styled, Window, div, px,
};

use crate::theme::{SurgicalTheme, SurgicalTypography, radius};

/// Port of `AWSProfile` (`Auth/AWSProfile.swift`), trimmed to the fields
/// this bead's helpers read. `sso` is collapsed to a `bool` since the full
/// `AWSSSOConfiguration` shape is out of scope here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwsProfile {
    pub name: String,
    pub access_key_id: Option<String>,
    pub sso: bool,
    pub region: Option<String>,
}

/// Static mock profile data standing in for `appState.
/// draftAvailableProfiles`. Real parsing is separate fleet auth work.
pub fn mock_profiles() -> Vec<AwsProfile> {
    vec![
        AwsProfile {
            name: "prod-admin".into(),
            access_key_id: Some("AKIA1234EXAMPLE".into()),
            sso: false,
            region: Some("us-east-1".into()),
        },
        AwsProfile {
            name: "staging-readonly".into(),
            access_key_id: None,
            sso: true,
            region: None,
        },
        AwsProfile {
            name: "sandbox".into(),
            access_key_id: None,
            sso: false,
            region: None,
        },
    ]
}

/// Port of `fuzzyMatch(query:value:)`: normalizes both strings (lowercase,
/// spaces stripped), then matches if `value` contains `query` as a
/// substring or `query`'s characters appear as an in-order subsequence of
/// `value`.
pub fn fuzzy_match(query: &str, value: &str) -> bool {
    let normalized_query: String = query
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let normalized_value: String = value
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    if normalized_query.is_empty() {
        return true;
    }
    if normalized_value.contains(&normalized_query) {
        return true;
    }

    let mut query_chars = normalized_query.chars();
    let mut current = query_chars.next();
    for c in normalized_value.chars() {
        if Some(c) == current {
            current = query_chars.next();
            if current.is_none() {
                return true;
            }
        }
    }
    false
}

/// Port of `profileIdentity(for:)`.
pub fn profile_identity(profile: &AwsProfile) -> String {
    if let Some(access_key_id) = &profile.access_key_id {
        if access_key_id.chars().count() >= 4 {
            let suffix: String = access_key_id
                .chars()
                .rev()
                .take(4)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            return format!("Access key ending in {suffix}");
        }
    }
    if profile.sso {
        return "SSO-backed profile".into();
    }
    "Shared credentials profile".into()
}

/// Port of `profileHintTag(for:)`.
pub fn profile_hint_tag(profile: &AwsProfile) -> &'static str {
    if profile.sso {
        return "SSO";
    }
    if profile.region.as_deref().is_some_and(|r| !r.is_empty()) {
        return "Configured region";
    }
    "Local profile"
}

/// Port of `preferredRegionText(for:)`: `profile.region ?? appState.
/// preferredRegion(for: profile.name)`. `preferred_region` stands in for
/// the `appState.preferredRegion(for:)` lookup, which doesn't exist in
/// this port yet.
pub fn preferred_region_text(profile: &AwsProfile, preferred_region: &str) -> String {
    profile
        .region
        .clone()
        .unwrap_or_else(|| preferred_region.to_string())
}

/// Port of `regionSourceLabel(for:)`. `remembered_region` stands in for
/// `appState.rememberedRegion(for:)`.
pub fn region_source_label(
    profile: &AwsProfile,
    remembered_region: Option<&str>,
    selected_region: &str,
) -> String {
    if let Some(configured) = profile.region.as_deref() {
        if !configured.is_empty() {
            return "Configured in profile".into();
        }
    }
    if let Some(remembered) = remembered_region {
        if !remembered.is_empty() {
            return "Remembered from previous use".into();
        }
    }
    format!("Falls back to {selected_region}")
}

/// Port of `regionLabel(for:)`.
pub fn region_label(region: &str) -> &str {
    match region {
        "us-east-1" => "US East (N. Virginia)",
        "us-east-2" => "US East (Ohio)",
        "us-west-1" => "US West (N. California)",
        "us-west-2" => "US West (Oregon)",
        "eu-west-1" => "EU West (Ireland)",
        "eu-central-1" => "EU Central (Frankfurt)",
        "ap-southeast-1" => "Asia Pacific (Singapore)",
        "ap-southeast-2" => "Asia Pacific (Sydney)",
        "ap-northeast-1" => "Asia Pacific (Tokyo)",
        _ => region,
    }
}

/// Port of `filteredProfiles`: profiles matching `query` (or all profiles,
/// if `query` is blank) by fuzzy-matching name, identity text, or region.
pub fn filtered_profiles(profiles: &[AwsProfile], query: &str) -> Vec<AwsProfile> {
    let query = query.trim();
    if query.is_empty() {
        return profiles.to_vec();
    }
    profiles
        .iter()
        .filter(|profile| {
            fuzzy_match(query, &profile.name)
                || fuzzy_match(query, &profile_identity(profile))
                || fuzzy_match(query, profile.region.as_deref().unwrap_or(""))
        })
        .cloned()
        .collect()
}

/// Port of the profile-search-and-select portion of
/// `AccountWorkspaceSetupStepView`: a search field over `profileRow`
/// entries, reporting the currently selected profile name. See module
/// docs for what's intentionally out of scope (the "Current Selection"
/// panel, region picker, error banner — `clouddeck-rs-w6s.9`).
pub struct AccountProfilePicker {
    pub profiles: Vec<AwsProfile>,
    pub search_text: String,
    pub selected_profile_name: Option<String>,
    /// Stand-in for `appState.preferredRegion(for:)`, used by
    /// `preferred_region_text` when a profile has no configured region.
    /// Real per-name lookups belong to whichever bead wires up the draft
    /// `AppState`.
    pub preferred_region_fallback: String,
}

impl Default for AccountProfilePicker {
    fn default() -> Self {
        Self {
            profiles: mock_profiles(),
            search_text: String::new(),
            selected_profile_name: None,
            preferred_region_fallback: "us-east-1".into(),
        }
    }
}

impl AccountProfilePicker {
    /// Port of the search field's `text` binding, mutated as the user
    /// types. gpui text-input wiring is out of scope for this bead (see
    /// `status_bar.rs`'s quick-picker search field for the same
    /// convention); tests drive this directly.
    pub fn set_search_text(&mut self, text: impl Into<String>) {
        self.search_text = text.into();
    }

    /// Port of the `profileRow` button action: `selectedProfileName =
    /// profile.name`.
    fn select_profile(&mut self, name: String) {
        self.selected_profile_name = Some(name);
    }

    /// Port of `filteredProfiles`, applied to this picker's own state.
    pub fn filtered_profiles(&self) -> Vec<AwsProfile> {
        filtered_profiles(&self.profiles, &self.search_text)
    }
}

impl Render for AccountProfilePicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let filtered = self.filtered_profiles();
        let selected_name = self.selected_profile_name.clone();
        let search_text = self.search_text.clone();
        let preferred_region_fallback = self.preferred_region_fallback.clone();

        div()
            .debug_selector(|| "account-workspace-profile-picker".into())
            .flex()
            .flex_col()
            .gap(px(14.0))
            .child(
                // searchField
                div()
                    .id("account-workspace-profile-search")
                    .debug_selector(|| "account-workspace-profile-search".into())
                    .px(px(11.0))
                    .py(px(8.0))
                    .rounded(px(radius::STANDARD))
                    .bg(SurgicalTheme::surface_container())
                    .text_size(px(SurgicalTypography::BODY_SIZE))
                    .text_color(if search_text.is_empty() {
                        SurgicalTheme::text_secondary()
                    } else {
                        SurgicalTheme::text_primary()
                    })
                    .child(if search_text.is_empty() {
                        "Search profiles".to_string()
                    } else {
                        search_text.clone()
                    }),
            )
            .child(
                // profilesPanel's row list
                div()
                    .id("account-workspace-profile-list")
                    .debug_selector(|| "account-workspace-profile-list".into())
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .p(px(6.0))
                    .rounded(px(radius::LARGE))
                    .bg(SurgicalTheme::surface())
                    .children(filtered.into_iter().map(|profile| {
                        let is_selected = selected_name.as_deref() == Some(profile.name.as_str());
                        let selector_id = format!("account-workspace-profile-row-{}", profile.name);
                        let debug_id = selector_id.clone();
                        let row_name = profile.name.clone();
                        let identity = profile_identity(&profile);
                        let hint_tag = profile_hint_tag(&profile);
                        let region_text =
                            preferred_region_text(&profile, &preferred_region_fallback);
                        // Port of `Image(systemName: isSelected ?
                        // "checkmark.circle.fill" : "circle")`: no
                        // gpui/cross-platform SF Symbol equivalent, so
                        // (mirroring `account_window.rs`'s
                        // `icon_placeholder`) this is a small filled/hollow
                        // dot standing in for the checkmark/circle glyph.
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
                                            .child(profile.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(SurgicalTypography::CAPTION_SIZE))
                                            .text_color(SurgicalTheme::text_secondary())
                                            .child(identity),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .items_end()
                                    .gap(px(3.0))
                                    .child(
                                        div()
                                            .text_size(px(SurgicalTypography::CAPTION_SIZE))
                                            .text_color(SurgicalTheme::text_primary())
                                            .child(region_text),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(SurgicalTypography::CAPTION_SIZE))
                                            .text_color(SurgicalTheme::text_secondary())
                                            .child(hint_tag),
                                    ),
                            )
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.select_profile(row_name.clone());
                                cx.notify();
                            }))
                    })),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[test]
    fn fuzzy_match_matches_substring_case_and_space_insensitively() {
        assert!(fuzzy_match("prod", "prod-admin"));
        assert!(fuzzy_match(
            "PROD ADMIN",
            "prod-admin".replace('-', " ").as_str()
        ));
        assert!(fuzzy_match("", "anything"));
    }

    #[test]
    fn fuzzy_match_matches_in_order_subsequence() {
        // "pdm" is a subsequence of "prod-admin" (p...d...m) but not a
        // substring.
        assert!(fuzzy_match("pdm", "prod-admin"));
        assert!(!fuzzy_match("mdp", "prod-admin"));
        assert!(!fuzzy_match("xyz", "prod-admin"));
    }

    #[test]
    fn profile_identity_prefers_access_key_then_sso_then_shared() {
        let with_key = AwsProfile {
            name: "a".into(),
            access_key_id: Some("AKIA1234EXAMPLE".into()),
            sso: true,
            region: None,
        };
        assert_eq!(profile_identity(&with_key), "Access key ending in MPLE");

        let sso_only = AwsProfile {
            name: "a".into(),
            access_key_id: None,
            sso: true,
            region: None,
        };
        assert_eq!(profile_identity(&sso_only), "SSO-backed profile");

        let shared = AwsProfile {
            name: "a".into(),
            access_key_id: None,
            sso: false,
            region: None,
        };
        assert_eq!(profile_identity(&shared), "Shared credentials profile");

        let short_key = AwsProfile {
            name: "a".into(),
            access_key_id: Some("AKI".into()),
            sso: false,
            region: None,
        };
        assert_eq!(profile_identity(&short_key), "Shared credentials profile");
    }

    #[test]
    fn profile_hint_tag_prefers_sso_then_region_then_local() {
        let sso = AwsProfile {
            name: "a".into(),
            access_key_id: None,
            sso: true,
            region: Some("us-east-1".into()),
        };
        assert_eq!(profile_hint_tag(&sso), "SSO");

        let regioned = AwsProfile {
            name: "a".into(),
            access_key_id: None,
            sso: false,
            region: Some("us-east-1".into()),
        };
        assert_eq!(profile_hint_tag(&regioned), "Configured region");

        let local = AwsProfile {
            name: "a".into(),
            access_key_id: None,
            sso: false,
            region: None,
        };
        assert_eq!(profile_hint_tag(&local), "Local profile");
    }

    #[test]
    fn preferred_region_text_falls_back_when_profile_has_no_region() {
        let regioned = AwsProfile {
            name: "a".into(),
            access_key_id: None,
            sso: false,
            region: Some("eu-west-1".into()),
        };
        assert_eq!(preferred_region_text(&regioned, "us-east-1"), "eu-west-1");

        let unregioned = AwsProfile {
            name: "a".into(),
            access_key_id: None,
            sso: false,
            region: None,
        };
        assert_eq!(preferred_region_text(&unregioned, "us-east-1"), "us-east-1");
    }

    #[test]
    fn region_source_label_prefers_configured_then_remembered_then_fallback() {
        let configured = AwsProfile {
            name: "a".into(),
            access_key_id: None,
            sso: false,
            region: Some("eu-west-1".into()),
        };
        assert_eq!(
            region_source_label(&configured, Some("us-west-2"), "us-east-1"),
            "Configured in profile"
        );

        let unregioned = AwsProfile {
            name: "a".into(),
            access_key_id: None,
            sso: false,
            region: None,
        };
        assert_eq!(
            region_source_label(&unregioned, Some("us-west-2"), "us-east-1"),
            "Remembered from previous use"
        );
        assert_eq!(
            region_source_label(&unregioned, None, "us-east-1"),
            "Falls back to us-east-1"
        );
    }

    #[test]
    fn helpers_treat_an_empty_region_string_the_same_as_no_region() {
        let empty_region = AwsProfile {
            name: "a".into(),
            access_key_id: None,
            sso: false,
            region: Some(String::new()),
        };
        assert_eq!(profile_hint_tag(&empty_region), "Local profile");
        assert_eq!(
            preferred_region_text(&empty_region, "us-east-1"),
            String::new(),
            "oracle's `??` only treats nil as missing, so an explicit empty string still wins here"
        );
        assert_eq!(
            region_source_label(&empty_region, Some("us-west-2"), "us-east-1"),
            "Remembered from previous use"
        );
    }

    #[test]
    fn region_label_maps_known_codes_and_falls_back_to_the_code() {
        assert_eq!(region_label("us-east-1"), "US East (N. Virginia)");
        assert_eq!(region_label("ap-northeast-1"), "Asia Pacific (Tokyo)");
        assert_eq!(region_label("mars-central-1"), "mars-central-1");
    }

    #[test]
    fn filtered_profiles_returns_all_when_query_is_blank() {
        let profiles = mock_profiles();
        assert_eq!(filtered_profiles(&profiles, "  ").len(), profiles.len());
    }

    #[test]
    fn filtered_profiles_matches_name_identity_or_region() {
        let profiles = mock_profiles();

        let by_name = filtered_profiles(&profiles, "sandbox");
        assert_eq!(by_name.len(), 1);
        assert_eq!(by_name[0].name, "sandbox");

        let by_region = filtered_profiles(&profiles, "us-east-1");
        assert_eq!(by_region.len(), 1);
        assert_eq!(by_region[0].name, "prod-admin");

        let by_identity = filtered_profiles(&profiles, "sso-backed");
        assert_eq!(by_identity.len(), 1);
        assert_eq!(by_identity[0].name, "staging-readonly");

        assert!(filtered_profiles(&profiles, "nonexistent").is_empty());
    }

    #[gpui::test]
    fn mounts_without_a_display(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| AccountProfilePicker::default());

        assert!(
            cx.debug_bounds("account-workspace-profile-picker")
                .is_some()
        );
        assert!(
            cx.debug_bounds("account-workspace-profile-search")
                .is_some()
        );
        assert!(cx.debug_bounds("account-workspace-profile-list").is_some());
    }

    #[gpui::test]
    fn renders_a_row_per_mock_profile(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| AccountProfilePicker::default());

        // Literal selectors (rather than leaking a formatted string per
        // profile) matching `mock_profiles()`'s names.
        for selector in [
            "account-workspace-profile-row-prod-admin",
            "account-workspace-profile-row-staging-readonly",
            "account-workspace-profile-row-sandbox",
        ] {
            assert!(
                cx.debug_bounds(selector).is_some(),
                "expected a row for selector {selector}"
            );
        }
    }

    #[gpui::test]
    fn setting_search_text_filters_the_rendered_rows(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| AccountProfilePicker::default());

        view.update(cx, |picker, cx| {
            picker.set_search_text("sandbox");
            cx.notify();
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("account-workspace-profile-row-sandbox")
                .is_some()
        );
        assert!(
            cx.debug_bounds("account-workspace-profile-row-prod-admin")
                .is_none(),
            "non-matching profile should not render a row"
        );
    }

    /// Mirrors `account_window.rs`'s click-simulation helper: simulates a
    /// click at the center of the element registered under `selector`.
    fn click_selector(cx: &mut gpui::VisualTestContext, selector: &'static str) {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} should render before it can be clicked"));
        let center = bounds.center();
        cx.simulate_click(center, Default::default());
    }

    #[gpui::test]
    fn clicking_a_row_reports_it_as_the_selected_profile(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| AccountProfilePicker::default());

        click_selector(cx, "account-workspace-profile-row-staging-readonly");

        view.update(cx, |picker, _cx| {
            assert_eq!(
                picker.selected_profile_name.as_deref(),
                Some("staging-readonly")
            );
        });
    }
}
