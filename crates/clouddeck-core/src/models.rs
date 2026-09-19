use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EC2InstanceTag {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct EC2SecurityGroup {
    pub group_id: Option<String>,
    pub group_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EC2Instance {
    pub instance_id: String,
    pub region: Option<String>,
    pub instance_type: Option<String>,
    pub state: Option<String>,
    pub launch_time: Option<SystemTime>,
    pub public_ip_address: Option<String>,
    pub private_ip_address: Option<String>,
    pub vpc_id: Option<String>,
    pub subnet_id: Option<String>,
    pub security_groups: Vec<EC2SecurityGroup>,
    pub tags: Vec<EC2InstanceTag>,
}

impl EC2Instance {
    /// `{region}:{instance_id}` when region is set and non-empty, else bare instance id.
    pub fn id(&self) -> String {
        match &self.region {
            Some(region) if !region.is_empty() => format!("{region}:{}", self.instance_id),
            _ => self.instance_id.clone(),
        }
    }

    /// Value of the first tag whose key case-insensitively equals "Name".
    pub fn name(&self) -> Option<&str> {
        self.tags
            .iter()
            .find(|tag| tag.key.eq_ignore_ascii_case("Name"))
            .map(|tag| tag.value.as_str())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComputeInstanceFilters {
    pub name_contains: Option<String>,
    pub states: Vec<String>,
    pub instance_types: Vec<String>,
}

impl ComputeInstanceFilters {
    /// Mirrors Swift's `Array<EC2Instance>.applying(filters:)`: all set
    /// conditions are AND'd, and empty fields impose no constraint.
    pub fn apply(&self, instances: &[EC2Instance]) -> Vec<EC2Instance> {
        let normalized_name_query = self
            .name_contains
            .as_deref()
            .map(|q| q.trim().to_lowercase());
        let normalized_states: std::collections::HashSet<String> = self
            .states
            .iter()
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        let normalized_types: std::collections::HashSet<String> = self
            .instance_types
            .iter()
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        instances
            .iter()
            .filter(|instance| {
                if let Some(query) = normalized_name_query.as_deref()
                    && !query.is_empty()
                {
                    let mut fields: Vec<&str> = vec![&instance.instance_id];
                    fields.extend(
                        [
                            instance.region.as_deref(),
                            instance.instance_type.as_deref(),
                            instance.state.as_deref(),
                            instance.public_ip_address.as_deref(),
                            instance.private_ip_address.as_deref(),
                            instance.vpc_id.as_deref(),
                            instance.subnet_id.as_deref(),
                        ]
                        .into_iter()
                        .flatten(),
                    );
                    for tag in &instance.tags {
                        fields.extend([tag.key.as_str(), tag.value.as_str()]);
                    }
                    for group in &instance.security_groups {
                        fields.extend(
                            [group.group_id.as_deref(), group.group_name.as_deref()]
                                .into_iter()
                                .flatten(),
                        );
                    }
                    let text = fields.join(" ").to_lowercase();
                    if !query.split_whitespace().all(|term| text.contains(term)) {
                        return false;
                    }
                }

                if !normalized_states.is_empty() {
                    match instance.state.as_deref() {
                        Some(state) if normalized_states.contains(&state.to_lowercase()) => {}
                        _ => return false,
                    }
                }

                if !normalized_types.is_empty() {
                    match instance.instance_type.as_deref() {
                        Some(instance_type)
                            if normalized_types.contains(&instance_type.to_lowercase()) => {}
                        _ => return false,
                    }
                }

                true
            })
            .cloned()
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputeServicePartialFailure {
    pub region: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ComputeInstanceLoadResult {
    pub instances: Vec<EC2Instance>,
    pub partial_failures: Vec<ComputeServicePartialFailure>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance(instance_id: &str, instance_type: &str, state: &str, name: &str) -> EC2Instance {
        EC2Instance {
            instance_id: instance_id.to_string(),
            region: None,
            instance_type: Some(instance_type.to_string()),
            state: Some(state.to_string()),
            launch_time: None,
            public_ip_address: None,
            private_ip_address: None,
            vpc_id: None,
            subnet_id: None,
            security_groups: vec![],
            tags: vec![EC2InstanceTag {
                key: "Name".to_string(),
                value: name.to_string(),
            }],
        }
    }

    #[test]
    fn id_includes_region_when_present_and_non_empty() {
        let mut inst = instance("i-1", "t3.micro", "running", "api");
        inst.region = Some("us-east-1".to_string());
        assert_eq!(inst.id(), "us-east-1:i-1");
    }

    #[test]
    fn id_falls_back_to_instance_id_without_region() {
        let inst = instance("i-1", "t3.micro", "running", "api");
        assert_eq!(inst.id(), "i-1");
    }

    #[test]
    fn id_falls_back_to_instance_id_when_region_empty() {
        let mut inst = instance("i-1", "t3.micro", "running", "api");
        inst.region = Some(String::new());
        assert_eq!(inst.id(), "i-1");
    }

    #[test]
    fn name_matches_tag_key_case_insensitively() {
        let mut inst = instance("i-1", "t3.micro", "running", "api");
        inst.tags = vec![EC2InstanceTag {
            key: "name".to_string(),
            value: "api-server".to_string(),
        }];
        assert_eq!(inst.name(), Some("api-server"));
    }

    #[test]
    fn name_is_none_without_matching_tag() {
        let mut inst = instance("i-1", "t3.micro", "running", "api");
        inst.tags = vec![];
        assert_eq!(inst.name(), None);
    }

    #[test]
    fn filters_apply_matches_swift_oracle_case() {
        let instances = vec![
            instance("i-api-running", "t3.micro", "running", "api-server"),
            instance("i-batch-stopped", "m5.large", "stopped", "batch-worker"),
        ];

        let filters = ComputeInstanceFilters {
            name_contains: Some("api".to_string()),
            states: vec!["running".to_string()],
            instance_types: vec!["t3.micro".to_string()],
        };

        let filtered = filters.apply(&instances);
        let ids: Vec<&str> = filtered.iter().map(|i| i.instance_id.as_str()).collect();
        assert_eq!(ids, vec!["i-api-running"]);
    }

    #[test]
    fn filters_with_no_constraints_return_everything() {
        let instances = vec![
            instance("i-1", "t3.micro", "running", "a"),
            instance("i-2", "m5.large", "stopped", "b"),
        ];
        let filtered = ComputeInstanceFilters::default().apply(&instances);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn search_requires_all_terms_across_network_tags_and_security_groups() {
        let mut target = instance("i-frontend", "t3.micro", "running", "public-api");
        target.region = Some("eu-west-1".to_string());
        target.private_ip_address = Some("10.12.0.7".to_string());
        target.security_groups = vec![EC2SecurityGroup {
            group_id: Some("sg-web".to_string()),
            group_name: Some("edge-prod".to_string()),
        }];
        target.tags.push(EC2InstanceTag {
            key: "Owner".to_string(),
            value: "Platform".to_string(),
        });

        let mut distractor = target.clone();
        distractor.instance_id = "i-backend".to_string();
        distractor.tags[1].value = "internal".to_string();

        let filters = ComputeInstanceFilters {
            name_contains: Some("  EU-WEST-1 platform sg-web  ".to_string()),
            ..Default::default()
        };
        let filtered = filters.apply(&[target, distractor]);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].instance_id, "i-frontend");
    }

    #[test]
    fn state_and_type_filters_trim_whitespace_and_ignore_case() {
        let instances = vec![instance("i-1", "t3.micro", "Running", "api")];
        let filters = ComputeInstanceFilters {
            states: vec!["  running ".into()],
            instance_types: vec![" T3.MICRO ".into()],
            ..Default::default()
        };
        assert_eq!(filters.apply(&instances).len(), 1);
    }
}
