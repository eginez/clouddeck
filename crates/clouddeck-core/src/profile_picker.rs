use std::collections::HashSet;

/// A configured AWS profile as shown in the profile picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileInput {
    /// Profile name as configured.
    pub name: String,
    /// Twelve-digit account id, shown grouped when well formed.
    pub account_id: Option<String>,
    /// Default region of the profile.
    pub region: Option<String>,
}

/// One profile row: name, unique two-letter badge and account/region subtitle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileRow {
    /// Profile name as configured.
    pub name: String,
    /// Two-letter uppercase badge, unique within the list.
    pub badge: String,
    /// Account id and region, joined with a middle dot.
    pub subtitle: String,
}

const BADGE_SEPARATORS: [char; 4] = ['-', '_', '.', ' '];

/// Builds picker rows in input order with unique badges.
pub fn profile_rows(profiles: &[ProfileInput]) -> Vec<ProfileRow> {
    let mut taken = HashSet::new();
    profiles
        .iter()
        .map(|profile| {
            let badge = unique_badge(&profile.name, &taken);
            taken.insert(badge.clone());
            ProfileRow {
                name: profile.name.clone(),
                badge,
                subtitle: profile_subtitle(profile),
            }
        })
        .collect()
}

/// Keeps rows matching the trimmed, case-insensitive filter; an empty filter keeps all rows.
pub fn filter_profile_rows(rows: &[ProfileRow], filter: &str) -> Vec<ProfileRow> {
    let needle = filter.trim().to_lowercase();
    rows.iter()
        .filter(|row| {
            needle.is_empty()
                || format!("{} {}", row.name, row.subtitle)
                    .to_lowercase()
                    .contains(&needle)
        })
        .cloned()
        .collect()
}

/// Empty-state text for a filter that matches no profile rows.
pub fn profile_picker_empty_text(filter: &str) -> String {
    format!("No profiles match “{}”.", filter.trim())
}

fn alphanumeric_words(name: &str) -> Vec<String> {
    name.split(|c| BADGE_SEPARATORS.contains(&c))
        .map(|part| {
            part.chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
        })
        .filter(|part| !part.is_empty())
        .collect()
}

fn primary_badge(name: &str) -> String {
    let words = alphanumeric_words(name);
    let badge: String = match words.as_slice() {
        [] => String::new(),
        [single] => single.chars().take(2).collect(),
        [first, second, ..] => first
            .chars()
            .take(1)
            .chain(second.chars().take(1))
            .collect(),
    };
    badge.to_ascii_uppercase()
}

fn unique_badge(name: &str, taken: &HashSet<String>) -> String {
    let primary = primary_badge(name);
    if !taken.contains(&primary) {
        return primary;
    }
    let alphanumerics: Vec<char> = name.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let (Some(&first), Some(&last)) = (alphanumerics.first(), alphanumerics.last()) else {
        return primary;
    };
    let first_last = format!("{first}{last}").to_ascii_uppercase();
    if !taken.contains(&first_last) {
        return first_last;
    }
    ('2'..='9')
        .map(|digit| format!("{first}{digit}").to_ascii_uppercase())
        .find(|candidate| !taken.contains(candidate))
        .unwrap_or(primary)
}

fn profile_subtitle(profile: &ProfileInput) -> String {
    let account = profile.account_id.as_deref().map(display_account_id);
    account
        .iter()
        .map(String::as_str)
        .chain(profile.region.as_deref())
        .filter(|part| !part.is_empty())
        .collect::<Vec<&str>>()
        .join(" · ")
}

fn display_account_id(account_id: &str) -> String {
    let is_twelve_digits =
        account_id.len() == 12 && account_id.bytes().all(|byte| byte.is_ascii_digit());
    if is_twelve_digits {
        format!(
            "{}-{}-{}",
            &account_id[0..4],
            &account_id[4..8],
            &account_id[8..12]
        )
    } else {
        account_id.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(name: &str, account_id: Option<&str>, region: Option<&str>) -> ProfileInput {
        ProfileInput {
            name: name.to_string(),
            account_id: account_id.map(str::to_string),
            region: region.map(str::to_string),
        }
    }

    fn sample_profiles() -> Vec<ProfileInput> {
        vec![
            input("prod-readonly", Some("111122223333"), Some("us-east-1")),
            input("staging-admin", Some("111122224444"), Some("us-west-2")),
            input("sandbox", Some("777788889999"), Some("eu-west-1")),
            input("data-platform", Some("101010101010"), None),
            input("security-audit", None, Some("eu-central-1")),
            input("legacy-2019", Some("12345"), None),
            input("x", None, None),
        ]
    }

    fn badges(rows: &[ProfileRow]) -> Vec<&str> {
        rows.iter().map(|row| row.badge.as_str()).collect()
    }

    fn names(rows: &[ProfileRow]) -> Vec<&str> {
        rows.iter().map(|row| row.name.as_str()).collect()
    }

    #[test]
    fn badges_are_unique_and_follow_name_rules() {
        let rows = profile_rows(&sample_profiles());
        assert_eq!(badges(&rows), ["PR", "SA", "SX", "DP", "ST", "L2", "X"]);
    }

    #[test]
    fn subtitles_format_account_and_region() {
        let rows = profile_rows(&sample_profiles());
        let subtitles: Vec<&str> = rows.iter().map(|row| row.subtitle.as_str()).collect();
        assert_eq!(
            subtitles,
            [
                "1111-2222-3333 · us-east-1",
                "1111-2222-4444 · us-west-2",
                "7777-8888-9999 · eu-west-1",
                "1010-1010-1010",
                "eu-central-1",
                "12345",
                "",
            ]
        );
    }

    #[test]
    fn colliding_badges_fall_back_to_first_and_last_then_digit() {
        let profiles = vec![
            input("alpha-one", None, None),
            input("alpha-other", None, None),
            input("a-o", None, None),
        ];
        let rows = profile_rows(&profiles);
        assert_eq!(badges(&rows), ["AO", "AR", "A2"]);
    }

    #[test]
    fn filter_matches_name_or_subtitle_case_insensitively() {
        let rows = profile_rows(&sample_profiles());
        assert_eq!(
            names(&filter_profile_rows(&rows, "prod")),
            ["prod-readonly"]
        );
        assert_eq!(
            names(&filter_profile_rows(&rows, "1111")),
            ["prod-readonly", "staging-admin"]
        );
        assert_eq!(
            names(&filter_profile_rows(&rows, "EU-")),
            ["sandbox", "security-audit"]
        );
        assert_eq!(filter_profile_rows(&rows, "").len(), 7);
        assert!(filter_profile_rows(&rows, "zz").is_empty());
    }

    #[test]
    fn empty_text_uses_trimmed_filter() {
        assert_eq!(profile_picker_empty_text(" zz "), "No profiles match “zz”.");
    }
}
