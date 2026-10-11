/// The regions the fleet view loads; empty only when no regions are available.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionScope {
    selected: Vec<String>,
}

impl RegionScope {
    /// Every available region selected.
    pub fn all(available: &[String]) -> Self {
        Self {
            selected: available.to_vec(),
        }
    }

    /// Keeps known regions in available order, or all regions when none remain.
    pub fn from_selected(selected: &[String], available: &[String]) -> Self {
        let mut kept: Vec<String> = Vec::new();
        for region in available {
            if selected.contains(region) && !kept.contains(region) {
                kept.push(region.clone());
            }
        }
        if kept.is_empty() {
            Self::all(available)
        } else {
            Self { selected: kept }
        }
    }

    /// Selected regions in available order.
    pub fn selected(&self) -> &[String] {
        &self.selected
    }

    /// True when every available region is selected.
    pub fn is_all(&self, available: &[String]) -> bool {
        available.iter().all(|region| self.is_selected(region))
    }

    /// Adds or removes a region, keeping at least one selected.
    pub fn toggle(&mut self, region: &str, available: &[String]) {
        if !is_available(region, available) {
            return;
        }
        if self.is_selected(region) {
            if self.selected.len() > 1 {
                self.selected.retain(|selected| selected != region);
            }
        } else {
            self.selected = available
                .iter()
                .filter(|candidate| candidate.as_str() == region || self.is_selected(candidate))
                .cloned()
                .collect();
        }
    }

    /// Selects every available region.
    pub fn select_all(&mut self, available: &[String]) {
        self.selected = available.to_vec();
    }

    /// Selects only the given region when it is available.
    pub fn select_only(&mut self, region: &str, available: &[String]) {
        if is_available(region, available) {
            self.selected = vec![region.to_string()];
        }
    }

    fn is_selected(&self, region: &str) -> bool {
        self.selected.iter().any(|selected| selected == region)
    }
}

fn is_available(region: &str, available: &[String]) -> bool {
    available.iter().any(|candidate| candidate == region)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn available() -> Vec<String> {
        ["us-east-1", "us-west-2", "eu-west-1", "eu-central-1"]
            .iter()
            .map(|region| region.to_string())
            .collect()
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn all_selects_every_region_in_order() {
        let available = available();
        let scope = RegionScope::all(&available);

        assert_eq!(scope.selected(), available.as_slice());
        assert!(scope.is_all(&available));
    }

    #[test]
    fn toggle_removes_and_readds_keeping_available_order() {
        let available = available();
        let mut scope = RegionScope::all(&available);

        scope.toggle("eu-west-1", &available);
        assert_eq!(
            scope.selected(),
            strings(&["us-east-1", "us-west-2", "eu-central-1"]).as_slice()
        );
        assert!(!scope.is_all(&available));

        scope.toggle("eu-west-1", &available);
        assert_eq!(scope.selected(), available.as_slice());
    }

    #[test]
    fn select_only_and_toggle_never_leave_selection_empty() {
        let available = available();
        let mut scope = RegionScope::all(&available);

        scope.select_only("us-west-2", &available);
        assert_eq!(scope.selected(), strings(&["us-west-2"]).as_slice());

        scope.toggle("us-west-2", &available);
        assert_eq!(scope.selected(), strings(&["us-west-2"]).as_slice());

        scope.toggle("eu-central-1", &available);
        assert_eq!(
            scope.selected(),
            strings(&["us-west-2", "eu-central-1"]).as_slice()
        );

        scope.toggle("us-east-1", &available);
        assert_eq!(
            scope.selected(),
            strings(&["us-east-1", "us-west-2", "eu-central-1"]).as_slice()
        );
    }

    #[test]
    fn select_all_restores_every_region() {
        let available = available();
        let mut scope = RegionScope::all(&available);
        scope.select_only("us-west-2", &available);

        scope.select_all(&available);

        assert_eq!(scope.selected(), available.as_slice());
        assert!(scope.is_all(&available));
    }

    #[test]
    fn from_selected_filters_dedupes_and_falls_back_to_all() {
        let available = available();

        let scope = RegionScope::from_selected(
            &strings(&["eu-west-1", "bogus", "us-east-1", "eu-west-1"]),
            &available,
        );
        assert_eq!(
            scope.selected(),
            strings(&["us-east-1", "eu-west-1"]).as_slice()
        );

        let scope = RegionScope::from_selected(&strings(&["bogus"]), &available);
        assert_eq!(scope.selected(), available.as_slice());

        let scope = RegionScope::from_selected(&[], &available);
        assert_eq!(scope.selected(), available.as_slice());
    }

    #[test]
    fn unknown_region_leaves_selection_unchanged() {
        let available = available();
        let mut scope = RegionScope::from_selected(&strings(&["us-west-2"]), &available);
        let before = scope.clone();

        scope.toggle("bogus", &available);
        assert_eq!(scope, before);

        scope.select_only("bogus", &available);
        assert_eq!(scope, before);
    }
}
