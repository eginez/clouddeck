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

/// One row of the region picker; a `region` of None is the All regions row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionPickerRow {
    pub label: String,
    pub region: Option<String>,
    pub checked: bool,
    pub count: usize,
}

/// Picker rows: the All regions row, then one row per available region, filtered by label.
pub fn region_picker_rows(
    scope: &RegionScope,
    available: &[String],
    counts: &[(String, usize)],
    filter: &str,
) -> Vec<RegionPickerRow> {
    let filter = filter.trim().to_lowercase();
    let mut rows = vec![RegionPickerRow {
        label: "All regions".to_string(),
        region: None,
        checked: scope.is_all(available),
        count: available
            .iter()
            .map(|region| region_count(region, counts))
            .sum(),
    }];
    rows.extend(available.iter().map(|region| RegionPickerRow {
        label: region.clone(),
        region: Some(region.clone()),
        checked: scope.is_selected(region),
        count: region_count(region, counts),
    }));
    rows.retain(|row| filter.is_empty() || row.label.to_lowercase().contains(&filter));
    rows
}

/// Empty-state text for a picker filter that matches no rows.
pub fn region_picker_empty_text(filter: &str) -> String {
    format!("No regions match “{}”.", filter.trim())
}

/// Applies a picker row activation: All regions selects everything, a region toggles.
pub fn activate_region_row(scope: &mut RegionScope, row: &RegionPickerRow, available: &[String]) {
    match &row.region {
        None => scope.select_all(available),
        Some(region) => scope.toggle(region, available),
    }
}

fn region_count(region: &str, counts: &[(String, usize)]) -> usize {
    counts
        .iter()
        .find(|(name, _)| name == region)
        .map_or(0, |(_, count)| *count)
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

    fn counts() -> Vec<(String, usize)> {
        [
            ("us-east-1", 7),
            ("us-west-2", 5),
            ("eu-west-1", 4),
            ("eu-central-1", 4),
        ]
        .iter()
        .map(|(region, count)| (region.to_string(), *count))
        .collect()
    }

    fn labels(rows: &[RegionPickerRow]) -> Vec<String> {
        rows.iter().map(|row| row.label.clone()).collect()
    }

    #[test]
    fn picker_rows_for_all_scope_show_counts() {
        let available = available();
        let scope = RegionScope::all(&available);

        let rows = region_picker_rows(&scope, &available, &counts(), "");

        assert_eq!(
            labels(&rows),
            strings(&[
                "All regions",
                "us-east-1",
                "us-west-2",
                "eu-west-1",
                "eu-central-1"
            ])
        );
        assert!(rows.iter().all(|row| row.checked));
        assert_eq!(
            rows.iter().map(|row| row.count).collect::<Vec<_>>(),
            vec![20, 7, 5, 4, 4]
        );
        assert_eq!(rows[0].region, None);
    }

    #[test]
    fn picker_rows_reflect_only_selection() {
        let available = available();
        let mut scope = RegionScope::all(&available);
        scope.select_only("us-west-2", &available);

        let rows = region_picker_rows(&scope, &available, &counts(), "");

        assert!(!rows[0].checked);
        let checked: Vec<&str> = rows
            .iter()
            .filter(|row| row.checked)
            .map(|row| row.label.as_str())
            .collect();
        assert_eq!(checked, vec!["us-west-2"]);
    }

    #[test]
    fn picker_filter_is_trimmed_and_case_insensitive() {
        let available = available();
        let scope = RegionScope::all(&available);
        let counts = counts();
        let rows_for =
            |filter: &str| labels(&region_picker_rows(&scope, &available, &counts, filter));

        assert_eq!(rows_for("eu"), strings(&["eu-west-1", "eu-central-1"]));
        assert_eq!(rows_for("US-"), strings(&["us-east-1", "us-west-2"]));
        assert_eq!(rows_for("all"), strings(&["All regions"]));
        assert!(rows_for("zz").is_empty());
        assert_eq!(rows_for("  eu-w "), strings(&["eu-west-1"]));
    }

    #[test]
    fn picker_count_is_zero_when_missing() {
        let available = available();
        let scope = RegionScope::all(&available);
        let counts: Vec<(String, usize)> = counts()
            .into_iter()
            .filter(|(region, _)| region.as_str() != "eu-west-1")
            .collect();

        let rows = region_picker_rows(&scope, &available, &counts, "");

        assert_eq!(rows[0].count, 16);
        assert_eq!(rows[3].count, 0);
    }

    #[test]
    fn picker_empty_text_quotes_trimmed_filter() {
        assert_eq!(region_picker_empty_text(" zz "), "No regions match “zz”.");
    }

    #[test]
    fn activating_rows_selects_all_or_toggles_region() {
        let available = available();
        let mut scope = RegionScope::from_selected(&strings(&["us-west-2"]), &available);
        let rows = region_picker_rows(&scope, &available, &counts(), "");

        activate_region_row(&mut scope, &rows[0], &available);
        assert_eq!(scope.selected(), available.as_slice());

        let mut scope = RegionScope::all(&available);
        let rows = region_picker_rows(&scope, &available, &counts(), "");
        activate_region_row(&mut scope, &rows[1], &available);
        assert_eq!(
            scope.selected(),
            strings(&["us-west-2", "eu-west-1", "eu-central-1"]).as_slice()
        );
    }
}
