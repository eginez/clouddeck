//! Shared fixture for the query engine beads. Later beads compute expected
//! values against exactly this data: do not change it.
use crate::instance::Instance;
use crate::models::{EC2Instance, EC2InstanceTag, EC2SecurityGroup};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Fixed "now" for the fixture: 2026-10-08T12:04:00Z.
pub fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_791_461_040)
}

/// The five shared fixture instances, in order.
pub fn fleet() -> Vec<EC2Instance> {
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
            tags: tags("ml-trainer-01", &[("Team", "data"), ("Owner", "esteban")]),
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

/// The shared fixture fleet in the neutral model.
pub fn fleet_instances() -> Vec<Instance> {
    fleet().iter().map(Instance::from).collect()
}

/// A provider record with only its region and id set.
pub fn bare_ec2(region: &str, instance_id: &str) -> EC2Instance {
    EC2Instance {
        instance_id: instance_id.to_string(),
        region: Some(region.to_string()),
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

/// The neutral form of [`bare_ec2`].
pub fn bare_instance(region: &str, instance_id: &str) -> Instance {
    Instance::from(&bare_ec2(region, instance_id))
}

/// A tag record with the given key and value.
pub fn tag(key: &str, value: &str) -> EC2InstanceTag {
    EC2InstanceTag {
        key: key.to_string(),
        value: value.to_string(),
    }
}

/// A running sample instance in us-east-1a, with `edit` applied before conversion.
pub fn running_with(edit: impl FnOnce(&mut EC2Instance)) -> Instance {
    let mut record = EC2Instance {
        availability_zone: Some("us-east-1a".to_string()),
        state: Some("running".to_string()),
        instance_type: Some("t3.medium".to_string()),
        launch_time: Some(rfc3339("2026-10-07T07:00:00Z")),
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
        ..bare_ec2("us-east-1", "i-0123456789abcdef0")
    };
    edit(&mut record);
    Instance::from(&record)
}

/// A stopped sample instance in us-east-1b, with `edit` applied before conversion.
pub fn stopped_with(edit: impl FnOnce(&mut EC2Instance)) -> Instance {
    let mut record = EC2Instance {
        instance_id: "i-0d2fc5662eb309e50".to_string(),
        availability_zone: Some("us-east-1b".to_string()),
        state: Some("stopped".to_string()),
        instance_type: Some("m6i.large".to_string()),
        launch_time: Some(rfc3339("2026-09-06T15:00:00Z")),
        private_ip_address: Some("10.0.32.55".to_string()),
        state_transition_reason: Some("User initiated (2026-10-01 09:12:15 GMT)".to_string()),
        tags: vec![tag("Name", "billing-worker-01")],
        ..bare_ec2("us-east-1", "i-0d2fc5662eb309e50")
    };
    edit(&mut record);
    Instance::from(&record)
}

/// A sample instance with no attributes, with `edit` applied before conversion.
pub fn blank_with(edit: impl FnOnce(&mut EC2Instance)) -> Instance {
    let mut record = bare_ec2("us-east-1", "i-0123456789abcdef0");
    edit(&mut record);
    Instance::from(&record)
}

fn rfc3339(text: &str) -> SystemTime {
    chrono::DateTime::parse_from_rfc3339(text)
        .unwrap()
        .with_timezone(&chrono::Utc)
        .into()
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
        assert_eq!(
            moment.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            "2026-10-08T12:04:00Z"
        );
    }
}
