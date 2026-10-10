//! Shared fixture for the query engine beads. Later beads compute expected
//! values against exactly this data: do not change it.
use crate::models::{EC2Instance, EC2InstanceTag, EC2SecurityGroup};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Fixed "now" for the fixture: 2026-10-08T12:04:00Z.
pub(crate) fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_791_461_040)
}

/// The five shared fixture instances, in order.
pub(crate) fn fleet() -> Vec<EC2Instance> {
    vec![
        EC2Instance {
            instance_id: "i-0000000000000001".to_string(),
            region: Some("us-east-1".to_string()),
            instance_type: Some("t3.medium".to_string()),
            state: Some("running".to_string()),
            launch_time: Some(utc(1_790_856_000)),
            public_ip_address: Some("54.80.2.5".to_string()),
            private_ip_address: Some("10.0.4.3".to_string()),
            vpc_id: Some("vpc-0aaa1111".to_string()),
            subnet_id: Some("subnet-0bbb2222".to_string()),
            security_groups: vec![EC2SecurityGroup {
                group_name: Some("web-sg".to_string()),
                group_id: Some("sg-0ccc3333".to_string()),
            }],
            tags: tags(
                "api-gateway-01",
                &[
                    ("Environment", "production"),
                    ("Team", "platform"),
                    ("Owner", "esteban"),
                ],
            ),
            availability_zone: Some("us-east-1a".to_string()),
            image_id: Some("ami-0ddd4444".to_string()),
            key_name: Some("ops-ed25519".to_string()),
            architecture: None,
            state_transition_reason: None,
            iam_instance_profile_arn: None,
            private_dns_name: None,
            public_dns_name: None,
        },
        EC2Instance {
            instance_id: "i-0000000000000002".to_string(),
            region: Some("us-east-1".to_string()),
            instance_type: Some("m6i.large".to_string()),
            state: Some("stopped".to_string()),
            launch_time: Some(utc(1_785_585_600)),
            public_ip_address: None,
            private_ip_address: Some("10.0.11.16".to_string()),
            vpc_id: None,
            subnet_id: None,
            security_groups: vec![EC2SecurityGroup {
                group_name: Some("api-sg".to_string()),
                group_id: Some("sg-0eee5555".to_string()),
            }],
            tags: tags(
                "billing-worker-02",
                &[
                    ("Environment", "production"),
                    ("Team", "payments"),
                    ("Owner", "mira"),
                    ("CostCenter", "cc-1004"),
                    ("aws:cloudformation:stack", "billing-stack"),
                ],
            ),
            availability_zone: Some("us-east-1b".to_string()),
            image_id: None,
            key_name: None,
            architecture: None,
            state_transition_reason: None,
            iam_instance_profile_arn: None,
            private_dns_name: None,
            public_dns_name: None,
        },
        EC2Instance {
            instance_id: "i-0000000000000003".to_string(),
            region: Some("us-west-2".to_string()),
            instance_type: Some("t3.small".to_string()),
            state: Some("pending".to_string()),
            launch_time: Some(utc(1_791_439_200)),
            public_ip_address: Some("54.87.79.124".to_string()),
            private_ip_address: Some("10.1.18.29".to_string()),
            vpc_id: None,
            subnet_id: None,
            security_groups: Vec::new(),
            tags: tags(
                "web-frontend-03",
                &[
                    ("Environment", "staging"),
                    ("Team", "search"),
                    ("Owner", "jonas"),
                ],
            ),
            availability_zone: Some("us-west-2c".to_string()),
            image_id: None,
            key_name: None,
            architecture: None,
            state_transition_reason: None,
            iam_instance_profile_arn: None,
            private_dns_name: None,
            public_dns_name: None,
        },
        EC2Instance {
            instance_id: "i-0000000000000004".to_string(),
            region: Some("us-west-2".to_string()),
            instance_type: Some("t3.micro".to_string()),
            state: Some("running".to_string()),
            launch_time: Some(utc(1_789_905_600)),
            public_ip_address: None,
            private_ip_address: Some("10.1.81.146".to_string()),
            vpc_id: None,
            subnet_id: None,
            security_groups: vec![EC2SecurityGroup {
                group_name: Some("bastion-sg".to_string()),
                group_id: Some("sg-0fff6666".to_string()),
            }],
            tags: Vec::new(),
            availability_zone: Some("us-west-2a".to_string()),
            image_id: None,
            key_name: Some("ops-rsa".to_string()),
            architecture: None,
            state_transition_reason: None,
            iam_instance_profile_arn: None,
            private_dns_name: None,
            public_dns_name: None,
        },
        EC2Instance {
            instance_id: "i-0000000000000005".to_string(),
            region: Some("eu-west-1".to_string()),
            instance_type: Some("g5.xlarge".to_string()),
            state: Some("stopped".to_string()),
            launch_time: None,
            public_ip_address: None,
            private_ip_address: None,
            vpc_id: None,
            subnet_id: None,
            security_groups: Vec::new(),
            tags: tags(
                "ml-trainer-01",
                &[("Team", "data"), ("Owner", "esteban")],
            ),
            availability_zone: Some("eu-west-1b".to_string()),
            image_id: None,
            key_name: None,
            architecture: None,
            state_transition_reason: None,
            iam_instance_profile_arn: None,
            private_dns_name: None,
            public_dns_name: None,
        },
    ]
}

/// UNIX_EPOCH + `secs` seconds.
fn utc(secs: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

/// Tags with the given `Name` first, followed by the extra key/value pairs.
fn tags(name: &str, extra: &[(&str, &str)]) -> Vec<EC2InstanceTag> {
    std::iter::once(("Name", name))
        .chain(extra.iter().copied())
        .map(|(key, value)| EC2InstanceTag {
            key: key.to_string(),
            value: value.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};

    #[test]
    fn fixture_shape() {
        let fleet = fleet();
        assert_eq!(fleet.len(), 5);
        assert!(fleet[3].tags.is_empty());
        assert!(fleet[4].launch_time.is_none());
        assert_eq!(fleet[1].name(), Some("billing-worker-02"));
    }

    #[test]
    fn now_is_2026_10_08t12_04_00z() {
        assert_eq!(now(), UNIX_EPOCH + Duration::from_secs(1_791_461_040));
        let moment: DateTime<Utc> = now().into();
        assert_eq!(moment.format("%Y-%m-%dT%H:%M:%SZ").to_string(), "2026-10-08T12:04:00Z");
    }
}
