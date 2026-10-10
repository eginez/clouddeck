use crate::models::EC2Instance;

/// Distinct tag keys that work as field names (e.g. `owner:esteban`),
/// skipping `Name` (any ASCII case) and `aws:`-prefixed keys, first
/// occurrence wins on case clashes, sorted ascending ignoring ASCII case.
pub fn known_tag_keys(instances: &[EC2Instance]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for instance in instances {
        for tag in &instance.tags {
            let key = &tag.key;
            if key.eq_ignore_ascii_case("Name") || key.starts_with("aws:") {
                continue;
            }
            if !seen.iter().any(|kept| kept.eq_ignore_ascii_case(key)) {
                seen.push(key.clone());
            }
        }
    }
    seen.sort_by(|a, b| a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()));
    seen
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{EC2Instance, EC2InstanceTag};
    use crate::query::fixtures;

    #[test]
    fn fixture_fleet_keys() {
        assert_eq!(
            known_tag_keys(&fixtures::fleet()),
            vec![
                "CostCenter".to_string(),
                "Environment".to_string(),
                "Owner".to_string(),
                "Team".to_string()
            ]
        );
    }

    #[test]
    fn empty_instances_have_no_keys() {
        assert!(known_tag_keys(&[]).is_empty());
    }

    #[test]
    fn first_seen_case_wins_and_name_and_aws_prefixed_are_skipped() {
        let instances = vec![
            tagged(&[("team", "a"), ("Team", "b")]),
            tagged(&[("NAME", "x"), ("aws:foo", "y")]),
        ];
        assert_eq!(known_tag_keys(&instances), vec!["team".to_string()]);
    }

    fn tagged(tags: &[(&str, &str)]) -> EC2Instance {
        EC2Instance {
            instance_id: "i-0000000000000009".to_string(),
            region: None,
            instance_type: None,
            state: None,
            launch_time: None,
            public_ip_address: None,
            private_ip_address: None,
            vpc_id: None,
            subnet_id: None,
            security_groups: Vec::new(),
            tags: tags
                .iter()
                .map(|(key, value)| EC2InstanceTag {
                    key: (*key).to_string(),
                    value: (*value).to_string(),
                })
                .collect(),
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
}
