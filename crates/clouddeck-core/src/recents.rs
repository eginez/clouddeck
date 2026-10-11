use crate::EC2Instance;

/// Recent searches shown in the empty search field.
pub const RECENT_SEARCHES_SHOWN: usize = 4;
/// Recent instances shown in the sidebar.
pub const RECENT_INSTANCES_SHOWN: usize = 3;
/// Default number of recent searches kept.
pub const DEFAULT_RECENT_SEARCH_LIMIT: usize = 10;
/// Number of recent instances kept.
pub const RECENT_INSTANCE_LIMIT: usize = 10;

/// Moves a trimmed item to the front of the list, dropping duplicates and capping its length.
pub fn push_recent(list: &mut Vec<String>, item: &str, limit: usize) {
    let trimmed = item.trim();
    if trimmed.is_empty() {
        return;
    }
    list.retain(|existing| existing != trimmed);
    list.insert(0, trimmed.to_string());
    list.truncate(limit);
}

/// Appends a trimmed query to the pins unless it is empty or already pinned.
pub fn pin_search(pins: &mut Vec<String>, query: &str) -> bool {
    let trimmed = query.trim();
    if trimmed.is_empty() || is_pinned(pins, trimmed) {
        return false;
    }
    pins.push(trimmed.to_string());
    true
}

/// Removes and returns the pin at `index`, or `None` when out of range.
pub fn unpin_search(pins: &mut Vec<String>, index: usize) -> Option<String> {
    (index < pins.len()).then(|| pins.remove(index))
}

/// Whether the trimmed query is among the pins.
pub fn is_pinned(pins: &[String], query: &str) -> bool {
    let trimmed = query.trim();
    pins.iter().any(|pin| pin == trimmed)
}

/// Looks up instances by `EC2Instance::id()` keys, in key order, skipping unknown keys.
pub fn resolve_recent_instances<'a>(
    keys: &[String],
    instances: &'a [EC2Instance],
) -> Vec<&'a EC2Instance> {
    keys.iter()
        .filter_map(|key| instances.iter().find(|instance| instance.id() == *key))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    fn instance(region: &str, instance_id: &str) -> EC2Instance {
        EC2Instance {
            instance_id: instance_id.to_string(),
            region: Some(region.to_string()),
            instance_type: None,
            state: Some("running".to_string()),
            launch_time: None,
            public_ip_address: None,
            private_ip_address: None,
            vpc_id: None,
            subnet_id: None,
            security_groups: vec![],
            tags: vec![],
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

    #[test]
    fn push_recent_orders_dedupes_trims_and_caps() {
        let mut list = Vec::new();
        push_recent(&mut list, "a", 3);
        assert_eq!(list, strings(&["a"]));
        push_recent(&mut list, "b", 3);
        assert_eq!(list, strings(&["b", "a"]));
        push_recent(&mut list, "a", 3);
        assert_eq!(list, strings(&["a", "b"]));
        push_recent(&mut list, "c", 3);
        assert_eq!(list, strings(&["c", "a", "b"]));
        push_recent(&mut list, "d", 3);
        assert_eq!(list, strings(&["d", "c", "a"]));
        push_recent(&mut list, "  ", 3);
        assert_eq!(list, strings(&["d", "c", "a"]));
        push_recent(&mut list, " c ", 3);
        assert_eq!(list, strings(&["c", "d", "a"]));
    }

    #[test]
    fn push_recent_with_zero_limit_empties_list() {
        let mut list = strings(&["a", "b"]);
        push_recent(&mut list, "x", 0);
        assert!(list.is_empty());
    }

    #[test]
    fn pin_search_trims_dedupes_and_appends() {
        let mut pins = Vec::new();
        assert!(pin_search(&mut pins, "state:stopped"));
        assert_eq!(pins, strings(&["state:stopped"]));
        assert!(!pin_search(&mut pins, " state:stopped "));
        assert!(!pin_search(&mut pins, ""));
        assert!(pin_search(&mut pins, "owner:esteban"));
        assert_eq!(pins, strings(&["state:stopped", "owner:esteban"]));
    }

    #[test]
    fn unpin_search_removes_by_index() {
        let mut pins = strings(&["state:stopped", "owner:esteban"]);
        assert_eq!(
            unpin_search(&mut pins, 0),
            Some("state:stopped".to_string())
        );
        assert_eq!(pins, strings(&["owner:esteban"]));
        assert_eq!(unpin_search(&mut pins, 5), None);
    }

    #[test]
    fn is_pinned_compares_trimmed_query() {
        let pins = strings(&["owner:esteban"]);
        assert!(is_pinned(&pins, " owner:esteban "));
        assert!(!is_pinned(&pins, "owner"));
    }

    #[test]
    fn resolve_recent_instances_follows_key_order_and_skips_missing() {
        let instances = vec![
            instance("us-east-1", "i-1"),
            instance("us-west-2", "i-2"),
            instance("eu-west-1", "i-3"),
        ];
        let keys = strings(&["eu-west-1:i-3", "missing:i-9", "us-east-1:i-1"]);
        let ids: Vec<&str> = resolve_recent_instances(&keys, &instances)
            .iter()
            .map(|instance| instance.instance_id.as_str())
            .collect();
        assert_eq!(ids, ["i-3", "i-1"]);
    }

    #[test]
    fn constants_match_design() {
        assert_eq!(RECENT_SEARCHES_SHOWN, 4);
        assert_eq!(RECENT_INSTANCES_SHOWN, 3);
        assert_eq!(DEFAULT_RECENT_SEARCH_LIMIT, 10);
        assert_eq!(RECENT_INSTANCE_LIMIT, 10);
    }
}
