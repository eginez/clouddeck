use crate::instance::Instance;

/// Distinct tag keys that work as field names (e.g. `owner:esteban`),
/// skipping `Name` (any ASCII case) and `aws:`-prefixed keys, first
/// occurrence wins on case clashes, sorted ascending ignoring ASCII case.
pub fn known_tag_keys(instances: &[Instance]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for instance in instances {
        for label in &instance.labels {
            let key = &label.key;
            if key.eq_ignore_ascii_case("Name") || key.starts_with("aws:") {
                continue;
            }
            if !seen.iter().any(|kept| kept.eq_ignore_ascii_case(key)) {
                seen.push(key.clone());
            }
        }
    }
    seen.sort_by_key(|key| key.to_ascii_lowercase());
    seen
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::Label;
    use crate::query::fixtures;

    #[test]
    fn fixture_fleet_keys() {
        assert_eq!(
            known_tag_keys(&fixtures::fleet_instances()),
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

    fn tagged(tags: &[(&str, &str)]) -> Instance {
        Instance {
            id: "i-0000000000000009".to_string(),
            labels: tags
                .iter()
                .map(|(key, value)| Label {
                    key: (*key).to_string(),
                    value: Some((*value).to_string()),
                })
                .collect(),
            ..Instance::default()
        }
    }
}
