pub use clouddeck_provider_api::{Address, AddressKind, ExtrasSection, Instance, Label};

use crate::models::EC2Instance;

impl From<&EC2Instance> for Instance {
    fn from(instance: &EC2Instance) -> Self {
        let mut addresses = Vec::new();
        if let Some(value) = non_empty(&instance.private_ip_address) {
            addresses.push(Address {
                kind: AddressKind::Private,
                value,
            });
        }
        if let Some(value) = non_empty(&instance.public_ip_address) {
            addresses.push(Address {
                kind: AddressKind::Public,
                value,
            });
        }

        let extras = vec![
            ExtrasSection {
                title: "Instance".to_string(),
                entries: vec![
                    entry("AMI", &instance.image_id),
                    entry("Key pair", &instance.key_name),
                    entry("IAM instance profile", &instance.iam_instance_profile_arn),
                ],
            },
            ExtrasSection {
                title: "Network".to_string(),
                entries: vec![
                    entry("Private DNS", &instance.private_dns_name),
                    entry("VPC", &instance.vpc_id),
                    entry("Subnet", &instance.subnet_id),
                    entry("Public DNS", &instance.public_dns_name),
                ],
            },
            ExtrasSection {
                title: "Security groups".to_string(),
                entries: instance
                    .security_groups
                    .iter()
                    .map(|group| {
                        (
                            group.group_name.clone().unwrap_or_default(),
                            group.group_id.clone().unwrap_or_default(),
                        )
                    })
                    .collect(),
            },
        ];

        Instance {
            id: instance.id(),
            name: instance
                .name()
                .filter(|name| !name.is_empty())
                .map(str::to_string),
            state: instance.state.clone(),
            state_reason: instance.state_transition_reason.clone(),
            instance_type: instance.instance_type.clone(),
            region: instance.region.clone(),
            location: instance.availability_zone.clone(),
            created: instance.launch_time,
            addresses,
            labels: instance
                .tags
                .iter()
                .map(|tag| Label {
                    key: tag.key.clone(),
                    value: tag.value.clone(),
                })
                .collect(),
            extras,
        }
    }
}

fn non_empty(value: &Option<String>) -> Option<String> {
    value.clone().filter(|value| !value.is_empty())
}

fn entry(key: &str, value: &Option<String>) -> (String, String) {
    (key.to_string(), value.clone().unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::fixtures::fleet;

    #[test]
    fn conversion_keeps_common_fields() {
        let source = &fleet()[0];
        let converted = Instance::from(source);

        assert_eq!(converted.id, "us-east-1:i-0000000000000001");
        assert_eq!(converted.name.as_deref(), Some("api-gateway-01"));
        assert_eq!(converted.state.as_deref(), Some("running"));
        assert_eq!(converted.instance_type.as_deref(), Some("t3.medium"));
        assert_eq!(converted.region.as_deref(), Some("us-east-1"));
        assert_eq!(converted.location.as_deref(), Some("us-east-1a"));
        assert_eq!(converted.created, source.launch_time);
        assert_eq!(
            converted.addresses,
            vec![
                Address {
                    kind: AddressKind::Private,
                    value: "10.0.4.3".to_string(),
                },
                Address {
                    kind: AddressKind::Public,
                    value: "54.80.2.5".to_string(),
                },
            ]
        );
        assert_eq!(converted.labels.len(), source.tags.len());
        assert_eq!(converted.labels[0].key, "Name");
        assert_eq!(converted.labels[0].value, "api-gateway-01");
    }

    #[test]
    fn conversion_keeps_state_reason() {
        let mut source = fleet()[0].clone();
        source.state_transition_reason =
            Some("User initiated (2026-10-03 09:12:15 GMT)".to_string());

        assert_eq!(
            Instance::from(&source).state_reason,
            source.state_transition_reason
        );
    }

    #[test]
    fn conversion_puts_aws_fields_into_extras_in_fixed_order() {
        let converted = Instance::from(&fleet()[0]);

        let titles: Vec<&str> = converted.extras.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["Instance", "Network", "Security groups"]);

        assert_eq!(
            converted.extras[0].entries,
            vec![
                ("AMI".to_string(), "ami-0ddd4444".to_string()),
                ("Key pair".to_string(), "ops-ed25519".to_string()),
                ("IAM instance profile".to_string(), String::new()),
            ]
        );
        assert_eq!(
            converted.extras[1].entries,
            vec![
                ("Private DNS".to_string(), String::new()),
                ("VPC".to_string(), "vpc-0aaa1111".to_string()),
                ("Subnet".to_string(), "subnet-0bbb2222".to_string()),
                ("Public DNS".to_string(), String::new()),
            ]
        );
        assert_eq!(
            converted.extras[2].entries,
            vec![("web-sg".to_string(), "sg-0ccc3333".to_string())]
        );
    }

    #[test]
    fn absent_public_ip_is_not_an_address() {
        let stopped = Instance::from(&fleet()[1]);
        assert_eq!(
            stopped.addresses,
            vec![Address {
                kind: AddressKind::Private,
                value: "10.0.11.16".to_string(),
            }]
        );

        let no_groups = Instance::from(&fleet()[2]);
        assert!(no_groups.extras[2].entries.is_empty());
    }
}
