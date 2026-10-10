//! Pure text helpers for the instance inspector pane.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::NaiveDateTime;

use crate::EC2Instance;

const STOP_TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S GMT";
const SECONDS_PER_HOUR: u64 = 3_600;
const SECONDS_PER_DAY: u64 = 86_400;

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
}
