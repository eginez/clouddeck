//! Pure text helpers for the instance inspector pane.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, NaiveDateTime, Utc};

use crate::EC2Instance;

const STOP_TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S GMT";
const LAUNCH_TIME_FORMAT: &str = "%Y-%m-%d %H:%M UTC";
const SECONDS_PER_HOUR: u64 = 3_600;
const SECONDS_PER_DAY: u64 = 86_400;

/// One label and value line in an inspector section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorRow {
    pub label: &'static str,
    pub value: Option<String>,
    pub empty_text: String,
    pub monospace: bool,
    pub copyable: bool,
}

/// A titled group of inspector rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorSection {
    pub title: String,
    pub rows: Vec<InspectorRow>,
}

/// Time parsed from the last parenthesised `YYYY-MM-DD HH:MM:SS GMT` in a state transition reason.
pub fn stopped_at(reason: &str) -> Option<SystemTime> {
    let open = reason.rfind('(')?;
    let close = reason.rfind(')')?;
    if close <= open {
        return None;
    }
    let naive =
        NaiveDateTime::parse_from_str(reason[open + 1..close].trim(), STOP_TIME_FORMAT).ok()?;
    let seconds = u64::try_from(naive.and_utc().timestamp()).ok()?;
    UNIX_EPOCH.checked_add(Duration::from_secs(seconds))
}

/// State transition reason without its trailing timestamp, trimmed.
pub fn reason_label(reason: &str) -> String {
    let label = match reason.rfind('(') {
        Some(open) if stopped_at(reason).is_some() => &reason[..open],
        _ => reason,
    };
    label.trim().to_string()
}

/// One-line summary shown under the instance name in the inspector.
pub fn inspector_summary(instance: &EC2Instance, now: SystemTime) -> String {
    let state = normalized_state(instance);
    if state.as_deref() == Some("stopped") {
        return stopped_summary(instance, now);
    }

    let mut parts = Vec::new();
    push_present(&mut parts, instance.instance_type.as_deref());
    push_present(&mut parts, instance.availability_zone.as_deref());
    match state.as_deref() {
        Some("running") => {
            if let Some(uptime) = instance.launch_time.and_then(|launch| uptime(launch, now)) {
                parts.push(format!("up {uptime}"));
            }
        }
        Some("pending") => parts.push("starting".to_string()),
        Some(other) => parts.push(other.to_string()),
        None => {}
    }
    parts.join(" · ")
}

/// Public address, or the reason there is none for the instance's state.
pub fn public_ip_text(instance: &EC2Instance) -> String {
    if let Some(ip) = instance
        .public_ip_address
        .as_deref()
        .filter(|ip| !ip.is_empty())
    {
        return ip.to_string();
    }
    match normalized_state(instance).as_deref() {
        Some("stopped") => "Released while stopped",
        Some("pending") => "Not assigned yet",
        _ => "No public address",
    }
    .to_string()
}

/// Instance and Network sections of the inspector body.
pub fn inspector_sections(instance: &EC2Instance) -> Vec<InspectorSection> {
    let instance_rows = vec![
        text_row(
            "State",
            capitalized(instance.state.as_deref()).as_deref(),
            false,
            false,
        ),
        text_row("Type", instance.instance_type.as_deref(), false, false),
        text_row("Zone", instance.availability_zone.as_deref(), false, false),
        text_row("AMI", instance.image_id.as_deref(), true, true),
        text_row("Key pair", instance.key_name.as_deref(), false, false),
        text_row(
            "Launched",
            instance.launch_time.map(launched_text).as_deref(),
            false,
            false,
        ),
    ];
    let network_rows = vec![
        text_row(
            "Private IP",
            instance.private_ip_address.as_deref(),
            true,
            true,
        ),
        InspectorRow {
            label: "Public IP",
            value: non_empty(instance.public_ip_address.as_deref()),
            empty_text: public_ip_text(instance),
            monospace: true,
            copyable: true,
        },
        text_row(
            "Private DNS",
            instance.private_dns_name.as_deref(),
            true,
            true,
        ),
        text_row("VPC", instance.vpc_id.as_deref(), true, true),
        text_row("Subnet", instance.subnet_id.as_deref(), true, true),
    ];
    vec![
        InspectorSection {
            title: "Instance".to_string(),
            rows: instance_rows,
        },
        InspectorSection {
            title: "Network".to_string(),
            rows: network_rows,
        },
    ]
}

/// Heading for the security group list, including the group count.
pub fn security_groups_title(instance: &EC2Instance) -> String {
    format!("Security groups · {}", instance.security_groups.len())
}

/// `(group name, group id)` for each security group, in model order; missing fields are empty.
pub fn security_group_rows(instance: &EC2Instance) -> Vec<(String, String)> {
    instance
        .security_groups
        .iter()
        .map(|group| {
            (
                group.group_name.clone().unwrap_or_default(),
                group.group_id.clone().unwrap_or_default(),
            )
        })
        .collect()
}

/// Tags shown before the inspector offers "Show all".
pub const TAGS_COLLAPSED_LIMIT: usize = 8;

/// Explanation shown in place of the tag list when an instance has no tags.
pub const NO_TAGS_TEXT: &str =
    "This instance has no tags. Names come from the Name tag, so it shows as unnamed.";

/// Name tag value for the inspector header, or `None` when it is absent or empty.
pub fn inspector_title(instance: &EC2Instance) -> Option<String> {
    instance
        .name()
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

/// All tags as `(key, value)`: Name first, then by key ignoring ASCII case, then by value.
pub fn inspector_tags(instance: &EC2Instance) -> Vec<(String, String)> {
    let mut tags: Vec<(String, String)> = instance
        .tags
        .iter()
        .map(|tag| (tag.key.clone(), tag.value.clone()))
        .collect();
    tags.sort_by_key(|(key, value)| {
        (
            !key.eq_ignore_ascii_case("Name"),
            key.to_ascii_lowercase(),
            value.clone(),
        )
    });
    tags
}

/// Heading for the tag section, including the tag count.
pub fn tags_title(count: usize) -> String {
    format!("Tags · {count}")
}

/// Leading tags to show, all of them when expanded or when there are few enough.
pub fn visible_tags(tags: &[(String, String)], show_all: bool) -> &[(String, String)] {
    if show_all || tags.len() <= TAGS_COLLAPSED_LIMIT {
        tags
    } else {
        &tags[..TAGS_COLLAPSED_LIMIT]
    }
}

/// Toggle link under the tag list, or `None` when the list is not collapsed.
pub fn tags_toggle_text(count: usize, show_all: bool) -> Option<String> {
    if count <= TAGS_COLLAPSED_LIMIT {
        None
    } else if show_all {
        Some("Show fewer".to_string())
    } else {
        Some(format!("Show all {count} tags"))
    }
}

/// Note shown when the instance's region failed on the last refresh.
pub fn stale_note(refreshed_at: &str, region: &str) -> String {
    format!("From the {refreshed_at} refresh. {region} failed to load.")
}

fn text_row(
    label: &'static str,
    value: Option<&str>,
    monospace: bool,
    copyable: bool,
) -> InspectorRow {
    InspectorRow {
        label,
        value: non_empty(value),
        empty_text: "None".to_string(),
        monospace,
        copyable,
    }
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value.filter(|value| !value.is_empty()).map(str::to_string)
}

fn capitalized(value: Option<&str>) -> Option<String> {
    let mut chars = value.filter(|value| !value.is_empty())?.chars();
    let first = chars.next()?;
    Some(first.to_uppercase().chain(chars).collect())
}

fn launched_text(launch: SystemTime) -> String {
    DateTime::<Utc>::from(launch)
        .format(LAUNCH_TIME_FORMAT)
        .to_string()
}

fn normalized_state(instance: &EC2Instance) -> Option<String> {
    instance
        .state
        .as_deref()
        .filter(|state| !state.is_empty())
        .map(str::to_ascii_lowercase)
}

fn stopped_summary(instance: &EC2Instance, now: SystemTime) -> String {
    let reason = instance
        .state_transition_reason
        .as_deref()
        .unwrap_or("")
        .trim();
    match stopped_at(reason) {
        Some(stopped) => {
            let days = now
                .duration_since(stopped)
                .map_or(0, |elapsed| elapsed.as_secs() / SECONDS_PER_DAY);
            let when = match days {
                0 => "today".to_string(),
                _ => format!("{} ago", plural(days, "day")),
            };
            format!("Stopped {when} · {}", reason_label(reason))
        }
        None if !reason.is_empty() => format!("Stopped · {reason}"),
        None => "Stopped".to_string(),
    }
}

fn uptime(launch: SystemTime, now: SystemTime) -> Option<String> {
    let seconds = now.duration_since(launch).ok()?.as_secs();
    Some(if seconds < SECONDS_PER_HOUR {
        plural((seconds / 60).max(1), "minute")
    } else if seconds < SECONDS_PER_DAY {
        plural(seconds / SECONDS_PER_HOUR, "hour")
    } else {
        plural(seconds / SECONDS_PER_DAY, "day")
    })
}

fn plural(count: u64, unit: &str) -> String {
    if count == 1 {
        format!("{count} {unit}")
    } else {
        format!("{count} {unit}s")
    }
}

fn push_present(parts: &mut Vec<String>, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        parts.push(value.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EC2InstanceTag, EC2SecurityGroup};

    fn at(rfc3339: &str) -> SystemTime {
        let seconds = chrono::DateTime::parse_from_rfc3339(rfc3339)
            .unwrap()
            .timestamp();
        UNIX_EPOCH + Duration::from_secs(u64::try_from(seconds).unwrap())
    }

    fn now() -> SystemTime {
        at("2026-10-08T12:04:00Z")
    }

    fn blank() -> EC2Instance {
        EC2Instance {
            instance_id: "i-0123456789abcdef0".to_string(),
            region: Some("us-east-1".to_string()),
            instance_type: None,
            state: None,
            launch_time: None,
            public_ip_address: None,
            private_ip_address: None,
            vpc_id: None,
            subnet_id: None,
            security_groups: Vec::new(),
            tags: Vec::new(),
            availability_zone: None,
            image_id: None,
            key_name: None,
            architecture: None,
            state_transition_reason: None,
            iam_instance_profile_arn: None,
            private_dns_name: None,
            public_dns_name: None,
        }
    }

    fn running() -> EC2Instance {
        EC2Instance {
            availability_zone: Some("us-east-1a".to_string()),
            state: Some("running".to_string()),
            instance_type: Some("t3.medium".to_string()),
            launch_time: Some(at("2026-10-07T07:00:00Z")),
            private_ip_address: Some("10.0.4.3".to_string()),
            private_dns_name: Some("ip-10-0-4-3.ec2.internal".to_string()),
            vpc_id: Some("vpc-098f0fdb8".to_string()),
            subnet_id: Some("subnet-0d99b9061".to_string()),
            image_id: Some("ami-027d083c0".to_string()),
            key_name: Some("ops-ed25519".to_string()),
            security_groups: vec![
                EC2SecurityGroup {
                    group_id: Some("sg-0d2fc5662".to_string()),
                    group_name: Some("web-sg".to_string()),
                },
                EC2SecurityGroup {
                    group_id: Some("sg-074bb5deb".to_string()),
                    group_name: Some("monitoring-sg".to_string()),
                },
            ],
            tags: vec![
                tag("Team", "platform"),
                tag("Name", "api-gateway-01"),
                tag("aws:cloudformation:stack", "api-gateway-stack"),
            ],
            ..blank()
        }
    }

    fn stopped() -> EC2Instance {
        EC2Instance {
            instance_id: "i-0d2fc5662eb309e50".to_string(),
            availability_zone: Some("us-east-1b".to_string()),
            state: Some("stopped".to_string()),
            instance_type: Some("m6i.large".to_string()),
            launch_time: Some(at("2026-09-06T15:00:00Z")),
            private_ip_address: Some("10.0.32.55".to_string()),
            state_transition_reason: Some("User initiated (2026-10-01 09:12:15 GMT)".to_string()),
            tags: vec![tag("Name", "billing-worker-01")],
            ..blank()
        }
    }

    fn tag(key: &str, value: &str) -> EC2InstanceTag {
        EC2InstanceTag {
            key: key.to_string(),
            value: value.to_string(),
        }
    }

    fn with_state(mut instance: EC2Instance, state: &str) -> EC2Instance {
        instance.state = Some(state.to_string());
        instance
    }

    #[test]
    fn stopped_at_parses_last_parenthesised_timestamp() {
        assert_eq!(
            stopped_at("User initiated (2026-10-01 09:12:15 GMT)"),
            Some(at("2026-10-01T09:12:15Z"))
        );
        assert_eq!(stopped_at("Instance initiated shutdown"), None);
        assert_eq!(stopped_at("User initiated (garbage)"), None);
    }

    #[test]
    fn reason_label_strips_parsable_timestamp() {
        assert_eq!(
            reason_label("User initiated (2026-10-01 09:12:15 GMT)"),
            "User initiated"
        );
        assert_eq!(
            reason_label(" Server.SpotInstanceTermination "),
            "Server.SpotInstanceTermination"
        );
    }

    #[test]
    fn running_summary_reports_uptime_units() {
        assert_eq!(
            inspector_summary(&running(), now()),
            "t3.medium · us-east-1a · up 1 day"
        );

        let launched = |launch: &str| EC2Instance {
            launch_time: Some(at(launch)),
            ..running()
        };
        assert_eq!(
            inspector_summary(&launched("2026-10-08T09:00:00Z"), now()),
            "t3.medium · us-east-1a · up 3 hours"
        );
        assert_eq!(
            inspector_summary(&launched("2026-10-08T11:00:00Z"), now()),
            "t3.medium · us-east-1a · up 1 hour"
        );
        assert_eq!(
            inspector_summary(&launched("2026-10-08T12:03:30Z"), now()),
            "t3.medium · us-east-1a · up 1 minute"
        );
        assert_eq!(
            inspector_summary(&launched("2026-09-28T12:04:00Z"), now()),
            "t3.medium · us-east-1a · up 10 days"
        );
    }

    #[test]
    fn running_summary_omits_missing_or_future_launch() {
        let no_launch = EC2Instance {
            launch_time: None,
            ..running()
        };
        assert_eq!(
            inspector_summary(&no_launch, now()),
            "t3.medium · us-east-1a"
        );

        let future = EC2Instance {
            launch_time: Some(at("2026-10-09T00:00:00Z")),
            ..running()
        };
        assert_eq!(inspector_summary(&future, now()), "t3.medium · us-east-1a");

        let no_type_or_launch = EC2Instance {
            instance_type: None,
            launch_time: None,
            ..running()
        };
        assert_eq!(inspector_summary(&no_type_or_launch, now()), "us-east-1a");
    }

    #[test]
    fn stopped_summary_uses_reason_and_age() {
        assert_eq!(
            inspector_summary(&stopped(), now()),
            "Stopped 7 days ago · User initiated"
        );

        let with_reason = |reason: &str| EC2Instance {
            state_transition_reason: Some(reason.to_string()),
            ..stopped()
        };
        assert_eq!(
            inspector_summary(
                &with_reason("User initiated (2026-10-08 09:00:00 GMT)"),
                now()
            ),
            "Stopped today · User initiated"
        );
        assert_eq!(
            inspector_summary(
                &with_reason("User initiated (2026-10-07 09:00:00 GMT)"),
                now()
            ),
            "Stopped 1 day ago · User initiated"
        );
        assert_eq!(
            inspector_summary(&with_reason("Instance initiated shutdown"), now()),
            "Stopped · Instance initiated shutdown"
        );

        let no_reason = EC2Instance {
            state_transition_reason: None,
            ..stopped()
        };
        assert_eq!(inspector_summary(&no_reason, now()), "Stopped");
    }

    #[test]
    fn pending_and_other_states_are_appended_lowercased() {
        assert_eq!(
            inspector_summary(&with_state(running(), "pending"), now()),
            "t3.medium · us-east-1a · starting"
        );
        assert_eq!(
            inspector_summary(&with_state(running(), "Stopping"), now()),
            "t3.medium · us-east-1a · stopping"
        );
    }

    #[test]
    fn public_ip_text_explains_missing_address() {
        assert_eq!(public_ip_text(&running()), "No public address");
        assert_eq!(public_ip_text(&stopped()), "Released while stopped");
        assert_eq!(
            public_ip_text(&with_state(running(), "pending")),
            "Not assigned yet"
        );

        let with_public = EC2Instance {
            public_ip_address: Some("54.1.2.3".to_string()),
            ..running()
        };
        assert_eq!(public_ip_text(&with_public), "54.1.2.3");

        assert_eq!(
            public_ip_text(&with_state(blank(), "terminated")),
            "No public address"
        );
    }

    fn labels(section: &InspectorSection) -> Vec<&'static str> {
        section.rows.iter().map(|row| row.label).collect()
    }

    fn values(section: &InspectorSection) -> Vec<Option<&str>> {
        section
            .rows
            .iter()
            .map(|row| row.value.as_deref())
            .collect()
    }

    #[test]
    fn sections_are_instance_then_network() {
        let sections = inspector_sections(&running());
        let titles: Vec<&str> = sections.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["Instance", "Network"]);
        assert_eq!(
            labels(&sections[0]),
            ["State", "Type", "Zone", "AMI", "Key pair", "Launched"]
        );
        assert_eq!(
            labels(&sections[1]),
            ["Private IP", "Public IP", "Private DNS", "VPC", "Subnet"]
        );
    }

    #[test]
    fn running_instance_values() {
        let sections = inspector_sections(&running());
        assert_eq!(
            values(&sections[0]),
            [
                Some("Running"),
                Some("t3.medium"),
                Some("us-east-1a"),
                Some("ami-027d083c0"),
                Some("ops-ed25519"),
                Some("2026-10-07 07:00 UTC"),
            ]
        );
    }

    #[test]
    fn running_network_values_and_public_ip_placeholder() {
        let sections = inspector_sections(&running());
        assert_eq!(
            values(&sections[1]),
            [
                Some("10.0.4.3"),
                None,
                Some("ip-10-0-4-3.ec2.internal"),
                Some("vpc-098f0fdb8"),
                Some("subnet-0d99b9061"),
            ]
        );
        let public_ip = &sections[1].rows[1];
        assert_eq!(public_ip.empty_text, "No public address");
        assert!(
            sections[1]
                .rows
                .iter()
                .all(|row| row.monospace && row.copyable)
        );
    }

    #[test]
    fn ami_is_monospace_and_copyable_state_is_neither() {
        let sections = inspector_sections(&running());
        let ami = &sections[0].rows[3];
        assert!(ami.monospace && ami.copyable);
        let state = &sections[0].rows[0];
        assert!(!state.monospace && !state.copyable);
    }

    #[test]
    fn stopped_instance_shows_placeholders() {
        let sections = inspector_sections(&stopped());
        let ami = &sections[0].rows[3];
        assert_eq!(ami.value, None);
        assert_eq!(ami.empty_text, "None");
        assert_eq!(sections[0].rows[4].value, None);
        assert_eq!(sections[1].rows[1].empty_text, "Released while stopped");
    }

    #[test]
    fn empty_key_name_is_no_value() {
        let instance = EC2Instance {
            key_name: Some(String::new()),
            ..running()
        };
        let sections = inspector_sections(&instance);
        assert_eq!(sections[0].rows[4].value, None);
    }

    #[test]
    fn security_group_title_and_rows() {
        assert_eq!(security_groups_title(&running()), "Security groups · 2");
        assert_eq!(
            security_group_rows(&running()),
            [
                ("web-sg".to_string(), "sg-0d2fc5662".to_string()),
                ("monitoring-sg".to_string(), "sg-074bb5deb".to_string()),
            ]
        );
        assert_eq!(security_groups_title(&stopped()), "Security groups · 0");
        assert!(security_group_rows(&stopped()).is_empty());
    }

    fn many_tags() -> EC2Instance {
        EC2Instance {
            tags: std::iter::once(tag("Name", "x"))
                .chain((1..=9).rev().map(|n| tag(&format!("k{n}"), &n.to_string())))
                .collect(),
            ..running()
        }
    }

    fn pairs(tags: &[(String, String)]) -> Vec<(&str, &str)> {
        tags.iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect()
    }

    #[test]
    fn title_is_non_empty_name_tag() {
        assert_eq!(
            inspector_title(&running()),
            Some("api-gateway-01".to_string())
        );
        let untagged = EC2Instance {
            tags: Vec::new(),
            ..running()
        };
        assert_eq!(inspector_title(&untagged), None);
        let empty_name = EC2Instance {
            tags: vec![tag("Name", "")],
            ..running()
        };
        assert_eq!(inspector_title(&empty_name), None);
    }

    #[test]
    fn tags_put_name_first_then_by_key_and_value() {
        let tags = inspector_tags(&running());
        assert_eq!(
            pairs(&tags),
            [
                ("Name", "api-gateway-01"),
                ("aws:cloudformation:stack", "api-gateway-stack"),
                ("Team", "platform"),
            ]
        );

        let mixed = EC2Instance {
            tags: vec![
                tag("b", "2"),
                tag("NAME", "n"),
                tag("A", "1"),
                tag("b", "1"),
            ],
            ..running()
        };
        let tags = inspector_tags(&mixed);
        assert_eq!(
            pairs(&tags),
            [("NAME", "n"), ("A", "1"), ("b", "1"), ("b", "2")]
        );
    }

    #[test]
    fn tags_title_includes_count() {
        assert_eq!(tags_title(3), "Tags · 3");
        assert_eq!(tags_title(0), "Tags · 0");
    }

    #[test]
    fn many_tags_collapse_to_eight_with_toggle() {
        let tags = inspector_tags(&many_tags());
        assert_eq!(tags.len(), 10);

        let collapsed = visible_tags(&tags, false);
        let keys: Vec<&str> = collapsed.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(keys, ["Name", "k1", "k2", "k3", "k4", "k5", "k6", "k7"]);
        assert_eq!(visible_tags(&tags, true).len(), 10);

        assert_eq!(
            tags_toggle_text(10, false),
            Some("Show all 10 tags".to_string())
        );
        assert_eq!(tags_toggle_text(10, true), Some("Show fewer".to_string()));
        assert_eq!(tags_toggle_text(8, false), None);
        assert_eq!(tags_toggle_text(0, false), None);
    }

    #[test]
    fn collapse_boundary_is_between_eight_and_nine_tags() {
        let tags_of = |count: usize| -> Vec<(String, String)> {
            (0..count)
                .map(|i| (format!("k{i}"), i.to_string()))
                .collect()
        };
        assert_eq!(visible_tags(&tags_of(8), false).len(), 8);
        assert_eq!(visible_tags(&tags_of(9), false).len(), 8);
        assert_eq!(tags_toggle_text(8, false), None);
        assert_eq!(
            tags_toggle_text(9, false),
            Some("Show all 9 tags".to_string())
        );
    }

    #[test]
    fn no_tags_text_matches_design() {
        assert_eq!(
            NO_TAGS_TEXT,
            "This instance has no tags. Names come from the Name tag, so it shows as unnamed."
        );
    }

    #[test]
    fn stale_note_names_refresh_time_and_region() {
        assert_eq!(
            stale_note("12:01", "eu-central-1"),
            "From the 12:01 refresh. eu-central-1 failed to load."
        );
    }
}
