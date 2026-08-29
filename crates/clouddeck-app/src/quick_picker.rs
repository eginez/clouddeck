//! Port of `CloudContextQuickPicker.swift` (the Swift CloudDeck oracle's
//! popover launched from the status bar for switching AWS profile/region)
//! to gpui.
//!
//! This module owns the picker's pure data/filtering logic (mock
//! profile/region data, search filtering, row-title fallbacks) so it can
//! be unit-tested without a display. The popover is *rendered* from
//! `status_bar.rs`, since it needs to mutate `CloudContextStatusBar`'s
//! `active_quick_picker`/`profile`/`region` state on selection (the "hook
//! point" that bead left for this one) and gpui click handlers need a
//! `Context<CloudContextStatusBar>` to do that.
//!
//! Wiring this to real `~/.aws/credentials` parsing is separate fleet auth
//! work; the profile/region lists here are static mock data, matching this
//! bead's scope. See
//! `~/src/cloudDeck/main/Sources/CloudDeck/Views/Shared/CloudContextQuickPicker.swift`
//! for the oracle this was ported from.

use crate::status_bar::FLEET_REGION_IDENTIFIER;

/// Port of `AWSProfile` (mock stand-in only — the real model lives in the
/// fleet auth work, out of scope here).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwsProfile {
    pub name: String,
    pub hint: String,
}

/// Port of `appState.availableProfiles`: static mock profile data.
pub fn mock_profiles() -> Vec<AwsProfile> {
    vec![
        AwsProfile {
            name: "prod-admin".into(),
            hint: "Production account · us-east-1".into(),
        },
        AwsProfile {
            name: "staging-readonly".into(),
            hint: "Staging account · us-west-2".into(),
        },
        AwsProfile {
            name: "sandbox".into(),
            hint: "Sandbox account · eu-central-1".into(),
        },
    ]
}

/// Port of `appState.recentProfileNames`: static mock recent-profile
/// history.
pub fn mock_recent_profile_names() -> Vec<String> {
    vec!["staging-readonly".into()]
}

/// Port of `appState.availableRegions`: static mock region list, with the
/// "Fleet (curated regions)" sentinel entry.
pub fn mock_regions() -> Vec<String> {
    vec![
        FLEET_REGION_IDENTIFIER.into(),
        "us-east-1".into(),
        "us-west-2".into(),
        "eu-central-1".into(),
        "ap-southeast-1".into(),
    ]
}

/// Port of `recentProfiles`: recent names, excluding whichever one is
/// currently selected.
pub fn recent_profiles(selected_profile: Option<&str>) -> Vec<String> {
    mock_recent_profile_names()
        .into_iter()
        .filter(|name| Some(name.as_str()) != selected_profile)
        .collect()
}

/// Port of `filteredProfiles`: profiles not already shown in the
/// current/recent sections, optionally filtered by a case-insensitive
/// substring match against name or hint.
pub fn filtered_profiles(selected_profile: Option<&str>, search_text: &str) -> Vec<AwsProfile> {
    let recents = recent_profiles(selected_profile);
    let query = search_text.trim().to_lowercase();

    mock_profiles()
        .into_iter()
        .filter(|p| Some(p.name.as_str()) != selected_profile)
        .filter(|p| !recents.contains(&p.name))
        .filter(|p| {
            query.is_empty()
                || p.name.to_lowercase().contains(&query)
                || p.hint.to_lowercase().contains(&query)
        })
        .collect()
}

/// Port of `filteredRegions`: regions filtered by a case-insensitive
/// substring match.
pub fn filtered_regions(search_text: &str) -> Vec<String> {
    let query = search_text.trim().to_lowercase();
    mock_regions()
        .into_iter()
        .filter(|r| query.is_empty() || r.to_lowercase().contains(&query))
        .collect()
}

/// Port of the region row's `region == AppState.fleetRegionIdentifier ?
/// "Fleet (curated regions)" : region` display title.
pub fn region_row_title(region: &str) -> String {
    if region == FLEET_REGION_IDENTIFIER {
        "Fleet (curated regions)".into()
    } else {
        region.into()
    }
}

/// Port of `title`.
pub fn picker_title(is_profile: bool) -> &'static str {
    if is_profile {
        "Switch Profile"
    } else {
        "Switch Region"
    }
}

/// Port of `searchPrompt`.
pub fn search_prompt(is_profile: bool) -> &'static str {
    if is_profile {
        "Search profiles"
    } else {
        "Search regions"
    }
}

/// Port of `footerMessage`.
pub fn footer_message(is_profile: bool) -> &'static str {
    if is_profile {
        "Quick account switch"
    } else {
        "Choose the active AWS region or fleet scope"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_profiles_excludes_the_selected_profile() {
        assert_eq!(recent_profiles(None), vec!["staging-readonly".to_string()]);
        assert_eq!(
            recent_profiles(Some("staging-readonly")),
            Vec::<String>::new()
        );
    }

    #[test]
    fn filtered_profiles_excludes_selected_and_recent() {
        let profiles = filtered_profiles(Some("prod-admin"), "");
        let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
        assert!(!names.contains(&"prod-admin"), "selected profile excluded");
        assert!(
            !names.contains(&"staging-readonly"),
            "recent profile excluded"
        );
        assert!(names.contains(&"sandbox"));
    }

    #[test]
    fn filtered_profiles_matches_name_or_hint_case_insensitively() {
        let by_name = filtered_profiles(None, "SANDBOX");
        assert_eq!(by_name.len(), 1);
        assert_eq!(by_name[0].name, "sandbox");

        let by_hint = filtered_profiles(None, "us-east-1");
        assert_eq!(by_hint.len(), 1);
        assert_eq!(by_hint[0].name, "prod-admin");

        let no_match = filtered_profiles(None, "nonexistent");
        assert!(no_match.is_empty());
    }

    #[test]
    fn filtered_regions_matches_case_insensitively() {
        assert_eq!(filtered_regions("").len(), mock_regions().len());

        let matches = filtered_regions("us-");
        assert_eq!(
            matches,
            vec!["us-east-1".to_string(), "us-west-2".to_string()]
        );

        assert!(filtered_regions("nonexistent").is_empty());
    }

    #[test]
    fn region_row_title_uses_fleet_label_for_the_sentinel() {
        assert_eq!(
            region_row_title(FLEET_REGION_IDENTIFIER),
            "Fleet (curated regions)"
        );
        assert_eq!(region_row_title("us-west-2"), "us-west-2");
    }

    #[test]
    fn copy_matches_oracle_per_kind() {
        assert_eq!(picker_title(true), "Switch Profile");
        assert_eq!(picker_title(false), "Switch Region");
        assert_eq!(search_prompt(true), "Search profiles");
        assert_eq!(search_prompt(false), "Search regions");
        assert_eq!(footer_message(true), "Quick account switch");
        assert_eq!(
            footer_message(false),
            "Choose the active AWS region or fleet scope"
        );
    }
}
