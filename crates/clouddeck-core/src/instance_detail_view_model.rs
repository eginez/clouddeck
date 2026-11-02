use crate::instance::{Instance, Label, native_id};
use chrono::{DateTime, Utc};

/// Rust port of the Swift oracle's `InstanceDetailViewModel`
/// (`ViewModels/EC2/InstanceDetailViewModel.swift`).
///
/// Plain state + read-only accessors, mirroring the Swift `@Observable`
/// class's computed properties. No gpui types are referenced here; a gpui
/// view owns one of these and reads its accessors when rendering.
pub struct InstanceDetailViewModel {
    instance: Instance,
}

impl InstanceDetailViewModel {
    pub fn new(instance: Instance) -> Self {
        Self { instance }
    }

    pub fn instance(&self) -> &Instance {
        &self.instance
    }

    /// The instance's `Name` tag, falling back to its instance ID (Swift:
    /// `displayName`).
    pub fn display_name(&self) -> String {
        self.instance
            .name
            .clone()
            .unwrap_or_else(|| native_id(&self.instance).to_string())
    }

    /// A human-readable launch time, or `"—"` when absent (Swift:
    /// `launchTimeDescription`).
    ///
    /// Swift formats via `Date.formatted(date: .abbreviated, time:
    /// .shortened)`, which is locale- and timezone-aware. This port renders
    /// a fixed abbreviated-date/short-time format in UTC instead, since a
    /// plain Rust struct has no access to the user's locale or timezone; the
    /// shape ("Apr 20, 2026 at 12:00 PM") matches the Swift oracle's en_US
    /// output, but the UTC clock time will differ from a Swift build's
    /// local-timezone display. Locale/timezone-aware rendering, if needed,
    /// belongs in the gpui view layer, not here.
    pub fn launch_time_description(&self) -> String {
        match self.instance.created {
            Some(launch_time) => {
                let datetime: DateTime<Utc> = launch_time.into();
                format!(
                    "{} at {}",
                    datetime.format("%b %-d, %Y"),
                    datetime.format("%-I:%M %p")
                )
            }
            None => "—".to_string(),
        }
    }

    /// The instance's tags sorted case-insensitively by key, then by value
    /// (Swift: `sortedTags`).
    pub fn sorted_tags(&self) -> Vec<Label> {
        let mut labels = self.instance.labels.clone();
        labels.sort_by(|a, b| {
            let key_cmp = a.key.to_lowercase().cmp(&b.key.to_lowercase());
            if key_cmp == std::cmp::Ordering::Equal {
                a.value
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .cmp(&b.value.as_deref().unwrap_or_default().to_lowercase())
            } else {
                key_cmp
            }
        });
        labels
    }

    /// Human-readable descriptions of the instance's security groups, or
    /// `["None"]` when there are none to describe (Swift:
    /// `securityGroupDescriptions`).
    pub fn security_group_descriptions(&self) -> Vec<String> {
        let groups = self
            .instance
            .extras
            .iter()
            .find(|section| section.title == "Security groups")
            .map(|section| section.entries.as_slice())
            .unwrap_or_default();
        let rows: Vec<String> = groups
            .iter()
            .filter_map(|(name, id)| {
                let id = Some(id.trim()).filter(|s| !s.is_empty());
                let name = Some(name.trim()).filter(|s| !s.is_empty());

                match (name, id) {
                    (Some(name), Some(id)) => Some(format!("{name} ({id})")),
                    (Some(name), None) => Some(name.to_string()),
                    (None, Some(id)) => Some(id.to_string()),
                    (None, None) => None,
                }
            })
            .collect();

        if rows.is_empty() {
            vec!["None".to_string()]
        } else {
            rows
        }
    }

    /// `value`, or `"—"` when it is `None` or empty (Swift: `valueOrDash`).
    pub fn value_or_dash(value: Option<&str>) -> String {
        match value {
            Some(v) if !v.is_empty() => v.to_string(),
            _ => "—".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::{ExtrasSection, State, StateKind};
    use std::time::{Duration, UNIX_EPOCH};

    fn base_instance() -> Instance {
        Instance {
            id: "us-east-1:i-1234567890abcdef0".to_string(),
            instance_type: Some("t3.medium".to_string()),
            state: State {
                kind: StateKind::Running,
                raw: "running".to_string(),
            },
            region: Some("us-east-1".to_string()),
            ..Instance::default()
        }
    }

    fn with_groups(groups: &[(&str, &str)]) -> Instance {
        Instance {
            extras: vec![ExtrasSection {
                title: "Security groups".to_string(),
                entries: groups
                    .iter()
                    .map(|(name, id)| (name.to_string(), id.to_string()))
                    .collect(),
            }],
            ..base_instance()
        }
    }

    #[test]
    fn display_name_uses_name_tag_when_present() {
        let instance = Instance {
            name: Some("api-server".to_string()),
            ..base_instance()
        };
        let vm = InstanceDetailViewModel::new(instance);
        assert_eq!(vm.display_name(), "api-server");
    }

    #[test]
    fn display_name_falls_back_to_instance_id_without_name_tag() {
        let vm = InstanceDetailViewModel::new(base_instance());
        assert_eq!(vm.display_name(), "i-1234567890abcdef0");
    }

    #[test]
    fn launch_time_description_is_dash_when_absent() {
        let vm = InstanceDetailViewModel::new(base_instance());
        assert_eq!(vm.launch_time_description(), "—");
    }

    #[test]
    fn launch_time_description_formats_present_launch_time() {
        // 2026-04-20T12:34:00Z
        let instance = Instance {
            created: Some(UNIX_EPOCH + Duration::from_secs(1776688440)),
            ..base_instance()
        };
        let vm = InstanceDetailViewModel::new(instance);
        assert_eq!(vm.launch_time_description(), "Apr 20, 2026 at 12:34 PM");
    }

    #[test]
    fn sorted_tags_orders_by_key_then_value_case_insensitively() {
        let label = |key: &str, value: &str| Label {
            key: key.to_string(),
            value: Some(value.to_string()),
        };
        let instance = Instance {
            labels: vec![
                label("env", "prod"),
                label("Name", "z-server"),
                label("name", "a-server"),
            ],
            ..base_instance()
        };
        let vm = InstanceDetailViewModel::new(instance);

        let sorted = vm.sorted_tags();
        let pairs: Vec<(&str, &str)> = sorted
            .iter()
            .map(|label| {
                (
                    label.key.as_str(),
                    label.value.as_deref().unwrap_or_default(),
                )
            })
            .collect();
        assert_eq!(
            pairs,
            vec![("env", "prod"), ("name", "a-server"), ("Name", "z-server"),]
        );
    }

    #[test]
    fn sorted_tags_is_empty_without_tags() {
        let vm = InstanceDetailViewModel::new(base_instance());
        assert!(vm.sorted_tags().is_empty());
    }

    #[test]
    fn security_group_descriptions_combines_name_and_id() {
        let vm = InstanceDetailViewModel::new(with_groups(&[(" web ", " sg-123 ")]));
        assert_eq!(vm.security_group_descriptions(), vec!["web (sg-123)"]);
    }

    #[test]
    fn security_group_descriptions_falls_back_to_name_only() {
        let vm = InstanceDetailViewModel::new(with_groups(&[("web", "")]));
        assert_eq!(vm.security_group_descriptions(), vec!["web"]);
    }

    #[test]
    fn security_group_descriptions_falls_back_to_id_only() {
        let vm = InstanceDetailViewModel::new(with_groups(&[("", "sg-123")]));
        assert_eq!(vm.security_group_descriptions(), vec!["sg-123"]);
    }

    #[test]
    fn security_group_descriptions_skips_blank_entries() {
        let vm = InstanceDetailViewModel::new(with_groups(&[("", "   ")]));
        assert_eq!(vm.security_group_descriptions(), vec!["None"]);
    }

    #[test]
    fn security_group_descriptions_is_none_without_groups() {
        let vm = InstanceDetailViewModel::new(base_instance());
        assert_eq!(vm.security_group_descriptions(), vec!["None"]);
    }

    #[test]
    fn security_group_descriptions_lists_multiple_groups_in_order() {
        let vm = InstanceDetailViewModel::new(with_groups(&[("web", "sg-1"), ("", "sg-2")]));
        assert_eq!(vm.security_group_descriptions(), vec!["web (sg-1)", "sg-2"]);
    }

    #[test]
    fn value_or_dash_returns_value_when_present_and_non_empty() {
        assert_eq!(
            InstanceDetailViewModel::value_or_dash(Some("us-east-1")),
            "us-east-1"
        );
    }

    #[test]
    fn value_or_dash_returns_dash_when_none() {
        assert_eq!(InstanceDetailViewModel::value_or_dash(None), "—");
    }

    #[test]
    fn value_or_dash_returns_dash_when_empty() {
        assert_eq!(InstanceDetailViewModel::value_or_dash(Some("")), "—");
    }

    #[test]
    fn value_or_dash_does_not_trim_whitespace_only() {
        // Matches the Swift oracle: `valueOrDash` only checks `isEmpty`, it
        // does not trim, so a whitespace-only value is returned as-is.
        assert_eq!(InstanceDetailViewModel::value_or_dash(Some("   ")), "   ");
    }
}
