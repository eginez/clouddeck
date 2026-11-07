use std::collections::{BTreeMap, BTreeSet};

use clouddeck_provider_api::{
    CellFailure, Connection, ConnectionId, DimValue, Dimension, DimensionId, ErrorKind, Instance,
    ListResult, ProviderError, ProviderInfo, Selection, ValueId,
};

use crate::Settings;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderEntry {
    pub info: ProviderInfo,
    pub signed_in: bool,
    pub bundled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionsState {
    Loading,
    NeedsSignIn,
    Failed(String),
    Ready,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderMenuRow {
    pub id: String,
    pub name: String,
    pub short_name: String,
    pub signed_in: bool,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DimensionRow {
    pub value: Option<ValueId>,
    pub label: String,
    pub depth: usize,
    pub checked: bool,
    pub implied: bool,
    pub count: usize,
    pub failed: bool,
}

pub const ALL_PROVIDERS: &str = "all";

pub fn default_selection(dimensions: &[Dimension]) -> Selection {
    let mut selection = Selection::new();
    for dim in dimensions {
        selection.0.insert(dim.id.clone(), default_values(dim));
    }
    selection
}

pub fn short_name(info: &ProviderInfo) -> String {
    short_name_for(&info.id, &info.name)
}

pub fn short_name_for(id: &str, name: &str) -> String {
    if name.chars().count() <= 4 {
        name.to_string()
    } else {
        id.to_uppercase()
    }
}

pub fn switch_active_provider(model: &mut ContextModel, settings: &mut Settings, id: &str) -> bool {
    if !model.switch_provider(id) {
        return false;
    }
    settings.active_provider = Some(id.to_string());
    true
}

pub fn connection_tooltip(connection_label: &str, shortcut: &str) -> String {
    format!("Switch {}  {shortcut}", connection_label.to_lowercase())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextModel {
    providers: Vec<ProviderEntry>,
    active: String,
    connections: Vec<Connection>,
    connection: Option<ConnectionId>,
    connections_state: ConnectionsState,
    dimensions: Vec<Dimension>,
    selection: Selection,
    counts: BTreeMap<(DimensionId, ValueId), usize>,
    failures: Vec<CellFailure>,
    kind_label: String,
}

impl ContextModel {
    pub fn new(providers: Vec<ProviderEntry>, persisted: Option<&str>, fallback: &str) -> Self {
        let registered = |id: &str| providers.iter().any(|p| p.info.id == id);
        let active = match persisted {
            Some(id) if id == ALL_PROVIDERS || registered(id) => id.to_string(),
            _ => providers
                .iter()
                .find(|p| p.bundled && p.signed_in)
                .map(|p| p.info.id.clone())
                .or_else(|| registered(fallback).then(|| fallback.to_string()))
                .or_else(|| providers.first().map(|p| p.info.id.clone()))
                .unwrap_or_default(),
        };
        Self {
            providers,
            active,
            connections: Vec::new(),
            connection: None,
            connections_state: ConnectionsState::Loading,
            dimensions: Vec::new(),
            selection: Selection::new(),
            counts: BTreeMap::new(),
            failures: Vec::new(),
            kind_label: "Instances".to_string(),
        }
    }

    pub fn active_provider(&self) -> &str {
        &self.active
    }

    pub fn active_info(&self) -> Option<&ProviderInfo> {
        self.providers
            .iter()
            .find(|p| p.info.id == self.active)
            .map(|p| &p.info)
    }

    pub fn provider_name(&self) -> Option<&str> {
        self.active_info().map(|info| info.name.as_str())
    }

    pub fn connection_label(&self) -> Option<&str> {
        self.active_info()
            .map(|info| info.connection_label.as_str())
    }

    pub fn shows_all_providers(&self) -> bool {
        self.active == ALL_PROVIDERS
    }

    pub fn provider_menu(&self) -> Vec<ProviderMenuRow> {
        let all = ProviderMenuRow {
            id: ALL_PROVIDERS.to_string(),
            name: "All providers".to_string(),
            short_name: "All".to_string(),
            signed_in: true,
            active: self.shows_all_providers(),
        };
        std::iter::once(all)
            .chain(self.providers.iter().map(|p| ProviderMenuRow {
                id: p.info.id.clone(),
                name: p.info.name.clone(),
                short_name: short_name(&p.info),
                signed_in: p.signed_in,
                active: p.info.id == self.active,
            }))
            .collect()
    }

    pub fn set_signed_in(&mut self, id: &str, signed_in: bool) {
        if let Some(entry) = self.providers.iter_mut().find(|p| p.info.id == id) {
            entry.signed_in = signed_in;
        }
    }

    pub fn set_kind_label(&mut self, label: &str) {
        self.kind_label = label.to_string();
    }

    pub fn switch_provider(&mut self, id: &str) -> bool {
        let selectable = id == ALL_PROVIDERS || self.providers.iter().any(|p| p.info.id == id);
        if id == self.active || !selectable {
            return false;
        }
        self.active = id.to_string();
        self.connections.clear();
        self.connection = None;
        self.connections_state = ConnectionsState::Loading;
        self.clear_dimensions();
        true
    }

    pub fn set_connections(&mut self, result: Result<Vec<Connection>, ProviderError>) {
        match result {
            Err(error) if matches!(error.kind, ErrorKind::Auth) => {
                self.connections.clear();
                self.connection = None;
                self.clear_dimensions();
                self.connections_state = ConnectionsState::NeedsSignIn;
            }
            Err(error) => {
                self.connections.clear();
                self.connection = None;
                self.clear_dimensions();
                self.connections_state = ConnectionsState::Failed(error.message);
            }
            Ok(list) => {
                let kept = self
                    .connection
                    .take()
                    .filter(|current| list.iter().any(|c| c.id == *current));
                self.connection = kept.or_else(|| list.first().map(|c| c.id.clone()));
                self.connections = list;
                self.connections_state = ConnectionsState::Ready;
            }
        }
    }

    pub fn connections_state(&self) -> &ConnectionsState {
        &self.connections_state
    }

    pub fn needs_sign_in(&self) -> bool {
        matches!(self.connections_state, ConnectionsState::NeedsSignIn)
    }

    pub fn connections(&self) -> &[Connection] {
        &self.connections
    }

    pub fn connection(&self) -> Option<&ConnectionId> {
        self.connection.as_ref()
    }

    pub fn select_connection(&mut self, id: &str) -> bool {
        let listed = self.connections.iter().any(|c| c.id.as_str() == id);
        let already = self.connection.as_ref().is_some_and(|c| c.as_str() == id);
        if !listed || already {
            return false;
        }
        self.connection = Some(ConnectionId::from(id));
        self.clear_dimensions();
        true
    }

    pub fn set_dimensions(&mut self, dimensions: Vec<Dimension>) {
        self.clear_dimensions();
        self.dimensions = dimensions;
        for dim in &self.dimensions {
            let values = default_values(dim);
            self.selection.0.insert(dim.id.clone(), values);
        }
    }

    pub fn dimensions(&self) -> &[Dimension] {
        &self.dimensions
    }

    pub fn selection(&self) -> Selection {
        self.selection.clone()
    }

    pub fn set_values(&mut self, dimension: &str, values: Vec<ValueId>) {
        let Some(dim) = self.dimension(dimension).cloned() else {
            return;
        };
        let mut values = normalize(&dim, values);
        if values.is_empty() && !dim.optional {
            values = default_values(&dim);
        }
        self.selection.0.insert(dim.id, values);
    }

    pub fn toggle(&mut self, dimension: &str, value: &ValueId) {
        let Some(dim) = self.dimension(dimension).cloned() else {
            return;
        };
        let current = self.selection.values(dimension).to_vec();
        let next: Vec<ValueId> = if self.is_checked(dimension, value) {
            if current.contains(value) {
                current.into_iter().filter(|v| v != value).collect()
            } else {
                let cover = ancestors(&dim, value)
                    .into_iter()
                    .find(|a| current.contains(a));
                match cover {
                    Some(cover) => {
                        let mut next: Vec<ValueId> =
                            current.into_iter().filter(|v| *v != cover).collect();
                        next.extend(expand_except(&dim, &cover, value));
                        next
                    }
                    None => current,
                }
            }
        } else if dim.multi {
            current
                .into_iter()
                .chain(std::iter::once(value.clone()))
                .collect()
        } else {
            vec![value.clone()]
        };
        self.set_values(dimension, next);
    }

    pub fn select_all(&mut self, dimension: &str) {
        let Some(dim) = self.dimension(dimension) else {
            return;
        };
        let values = roots(dim);
        self.set_values(dimension, values);
    }

    pub fn is_checked(&self, dimension: &str, value: &ValueId) -> bool {
        let Some(dim) = self.dimension(dimension) else {
            return false;
        };
        let selected = self.selection.values(dimension);
        selected.contains(value) || ancestors(dim, value).iter().any(|a| selected.contains(a))
    }

    pub fn is_all(&self, dimension: &str) -> bool {
        let Some(dim) = self.dimension(dimension) else {
            return true;
        };
        self.selection.values(dimension).is_empty()
            || roots(dim)
                .iter()
                .all(|root| self.is_checked(dimension, root))
    }

    pub fn implied_parents(&self, dimension: &str) -> Vec<ValueId> {
        let Some(dim) = self.dimension(dimension) else {
            return Vec::new();
        };
        let mut implied: Vec<ValueId> = Vec::new();
        for value in self.selection.values(dimension) {
            for ancestor in ancestors(dim, value) {
                if !self.is_checked(dimension, &ancestor) && !implied.contains(&ancestor) {
                    implied.push(ancestor);
                }
            }
        }
        implied
    }

    pub fn set_list_result(
        &mut self,
        result: &ListResult,
        candidates: impl Fn(&Instance) -> Vec<ValueId>,
    ) {
        self.counts.clear();
        self.failures = result.failures.clone();
        let placements: Vec<Vec<ValueId>> = result.resources.iter().map(&candidates).collect();
        for dim in &self.dimensions {
            for placed in &placements {
                let Some(value) = placed
                    .iter()
                    .find(|candidate| dim.values.iter().any(|v| v.id == **candidate))
                else {
                    continue;
                };
                let mut hits = ancestors(dim, value);
                hits.push(value.clone());
                for hit in hits {
                    *self.counts.entry((dim.id.clone(), hit)).or_insert(0) += 1;
                }
            }
        }
    }

    pub fn value_count(&self, dimension: &str, value: &ValueId) -> usize {
        self.counts
            .get(&(DimensionId::from(dimension), value.clone()))
            .copied()
            .unwrap_or(0)
    }

    pub fn failed(&self, dimension: &str, value: &ValueId) -> bool {
        let Some(dim) = self.dimension(dimension) else {
            return false;
        };
        self.failures.iter().any(|failure| {
            failure
                .cell
                .values(dimension)
                .iter()
                .any(|cell| cell == value || ancestors(dim, cell).contains(value))
        })
    }

    pub fn whole_failure(&self) -> Option<&CellFailure> {
        self.failures.iter().find(|failure| failure.cell.is_empty())
    }

    pub fn rows(&self, dimension: &str, filter: &str) -> Vec<DimensionRow> {
        let Some(dim) = self.dimension(dimension) else {
            return Vec::new();
        };
        let needle = filter.trim().to_lowercase();
        let all_label = format!("All {}", dim.plural().to_lowercase());
        let all_count: usize = roots(dim)
            .iter()
            .map(|root| self.value_count(dimension, root))
            .sum();
        let implied = self.implied_parents(dimension);
        let mut rows = Vec::new();
        if label_matches(&all_label, &needle) {
            rows.push(DimensionRow {
                value: None,
                label: all_label,
                depth: 0,
                checked: self.is_all(dimension),
                implied: false,
                count: all_count,
                failed: self.whole_failure().is_some(),
            });
        }
        for root in dim.values.iter().filter(|v| v.parent.is_none()) {
            self.push_rows(dim, root, 0, &needle, &implied, &mut rows);
        }
        rows
    }

    pub fn summary(&self, dimension: &str) -> String {
        let Some(dim) = self.dimension(dimension) else {
            return String::new();
        };
        let plural = dim.plural().to_lowercase();
        if self.is_all(dimension) {
            return format!("All {plural}");
        }
        let selected = self.selection.values(dimension);
        if let [only] = selected
            && let Some(value) = dim.values.iter().find(|v| v.id == *only)
        {
            return value.label.clone();
        }
        format!("{} {plural}", selected.len())
    }

    pub fn count_text(&self, total: usize) -> String {
        let noun = self.kind_label.to_lowercase();
        let noun = if total == 1 {
            noun.strip_suffix('s').unwrap_or(&noun).to_string()
        } else {
            noun
        };
        format!("{total} {noun}")
    }

    fn dimension(&self, id: &str) -> Option<&Dimension> {
        self.dimensions.iter().find(|d| d.id.as_str() == id)
    }

    fn clear_dimensions(&mut self) {
        self.dimensions.clear();
        self.selection = Selection::new();
        self.counts.clear();
        self.failures.clear();
    }

    fn push_rows(
        &self,
        dim: &Dimension,
        value: &DimValue,
        depth: usize,
        needle: &str,
        implied: &[ValueId],
        rows: &mut Vec<DimensionRow>,
    ) {
        if !subtree_matches(dim, value, needle) {
            return;
        }
        rows.push(DimensionRow {
            value: Some(value.id.clone()),
            label: value.label.clone(),
            depth,
            checked: self.is_checked(dim.id.as_str(), &value.id),
            implied: implied.contains(&value.id),
            count: self.value_count(dim.id.as_str(), &value.id),
            failed: self.failed(dim.id.as_str(), &value.id),
        });
        for child in children_of(dim, &value.id) {
            self.push_rows(dim, child, depth + 1, needle, implied, rows);
        }
    }
}

fn label_matches(label: &str, needle: &str) -> bool {
    needle.is_empty() || label.to_lowercase().contains(needle)
}

fn subtree_matches(dim: &Dimension, value: &DimValue, needle: &str) -> bool {
    label_matches(&value.label, needle)
        || children_of(dim, &value.id)
            .into_iter()
            .any(|child| subtree_matches(dim, child, needle))
}

fn roots(dim: &Dimension) -> Vec<ValueId> {
    dim.values
        .iter()
        .filter(|v| v.parent.is_none())
        .map(|v| v.id.clone())
        .collect()
}

fn children_of<'a>(dim: &'a Dimension, id: &ValueId) -> Vec<&'a DimValue> {
    dim.values
        .iter()
        .filter(|v| v.parent.as_ref() == Some(id))
        .collect()
}

fn ancestors(dim: &Dimension, value: &ValueId) -> Vec<ValueId> {
    let mut out: Vec<ValueId> = Vec::new();
    let mut current = value.clone();
    loop {
        let parent = dim
            .values
            .iter()
            .find(|v| v.id == current)
            .and_then(|v| v.parent.clone());
        match parent {
            Some(parent) if parent != *value && !out.contains(&parent) => {
                out.push(parent.clone());
                current = parent;
            }
            _ => return out,
        }
    }
}

fn expand_except(dim: &Dimension, node: &ValueId, target: &ValueId) -> Vec<ValueId> {
    let target_ancestors = ancestors(dim, target);
    let mut out = Vec::new();
    for child in children_of(dim, node) {
        if child.id == *target {
            continue;
        }
        if target_ancestors.contains(&child.id) {
            out.extend(expand_except(dim, &child.id, target));
        } else {
            out.push(child.id.clone());
        }
    }
    out
}

fn default_values(dim: &Dimension) -> Vec<ValueId> {
    let defaults: Vec<ValueId> = dim
        .values
        .iter()
        .filter(|v| v.default)
        .map(|v| v.id.clone())
        .collect();
    let values = if !defaults.is_empty() {
        defaults
    } else if dim.optional {
        Vec::new()
    } else {
        roots(dim)
    };
    if dim.multi {
        values
    } else {
        values.into_iter().take(1).collect()
    }
}

fn normalize(dim: &Dimension, values: Vec<ValueId>) -> Vec<ValueId> {
    let mut seen: BTreeSet<ValueId> = BTreeSet::new();
    let mut kept: Vec<ValueId> = values
        .into_iter()
        .filter(|v| dim.values.iter().any(|d| d.id == *v) && seen.insert(v.clone()))
        .collect();
    if !dim.multi {
        kept = kept.last().cloned().into_iter().collect();
    }
    let snapshot = kept.clone();
    kept.retain(|v| !ancestors(dim, v).iter().any(|a| snapshot.contains(a)));
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(id: &str, name: &str, connection_label: &str) -> ProviderInfo {
        ProviderInfo {
            id: id.to_string(),
            name: name.to_string(),
            version: "1.0.0".to_string(),
            connection_label: connection_label.to_string(),
        }
    }

    fn entry(
        id: &str,
        name: &str,
        connection_label: &str,
        bundled: bool,
        signed_in: bool,
    ) -> ProviderEntry {
        ProviderEntry {
            info: info(id, name, connection_label),
            signed_in,
            bundled,
        }
    }

    fn value(id: &str, label: &str, parent: Option<&str>, default: bool) -> DimValue {
        DimValue {
            id: id.into(),
            label: label.to_string(),
            parent: parent.map(ValueId::from),
            default,
        }
    }

    fn dimension(
        id: &str,
        label: &str,
        multi: bool,
        optional: bool,
        values: Vec<DimValue>,
    ) -> Dimension {
        Dimension {
            id: id.into(),
            label: label.to_string(),
            plural_label: None,
            multi,
            optional,
            values,
        }
    }

    fn region_dimension() -> Dimension {
        dimension(
            "region",
            "Region",
            true,
            false,
            vec![
                value("us-east-1", "us-east-1", None, false),
                value("us-west-2", "us-west-2", None, false),
                value("eu-west-1", "eu-west-1", None, false),
            ],
        )
    }

    fn tree_dimension() -> Dimension {
        dimension(
            "region",
            "Region",
            true,
            false,
            vec![
                value("us", "US", None, false),
                value("us-east-1", "us-east-1", Some("us"), false),
                value("us-west-2", "us-west-2", Some("us"), false),
                value("eu", "EU", None, false),
            ],
        )
    }

    fn connection(id: &str) -> Connection {
        Connection {
            id: ConnectionId::from(id),
            display_name: id.to_string(),
        }
    }

    fn model_with_region(dim: Dimension) -> ContextModel {
        let mut model = ContextModel::new(
            vec![entry("aws", "AWS", "Profile", true, true)],
            None,
            "aws",
        );
        model.set_connections(Ok(vec![connection("default")]));
        model.set_dimensions(vec![dim]);
        model
    }

    fn ids(values: &[&str]) -> Vec<ValueId> {
        values.iter().map(|v| ValueId::from(*v)).collect()
    }

    #[test]
    fn default_active_provider_prefers_persisted_then_bundled_signed_in_then_fallback() {
        let providers = || {
            vec![
                entry("demo", "Demo", "Profile", false, true),
                entry("aws", "AWS", "Profile", true, true),
                entry("gcp", "Google Cloud", "Project", true, false),
            ]
        };
        assert_eq!(
            ContextModel::new(providers(), Some("gcp"), "aws").active_provider(),
            "gcp"
        );
        assert_eq!(
            ContextModel::new(providers(), Some("nope"), "gcp").active_provider(),
            "aws"
        );
        assert_eq!(
            ContextModel::new(providers(), None, "gcp").active_provider(),
            "aws"
        );

        let unsigned = vec![
            entry("demo", "Demo", "Profile", false, true),
            entry("gcp", "Google Cloud", "Project", true, false),
        ];
        assert_eq!(
            ContextModel::new(unsigned.clone(), None, "gcp").active_provider(),
            "gcp"
        );
        assert_eq!(
            ContextModel::new(unsigned, None, "missing").active_provider(),
            "demo"
        );
    }

    #[test]
    fn switch_provider_resets_connection_and_selection_and_reports_reload() {
        let mut model = ContextModel::new(
            vec![
                entry("aws", "AWS", "Profile", true, true),
                entry("gcp", "Google Cloud", "Project", true, true),
            ],
            None,
            "aws",
        );
        model.set_connections(Ok(vec![connection("default")]));
        model.set_dimensions(vec![region_dimension()]);
        model.set_values("region", ids(&["us-east-1"]));

        assert!(model.switch_provider("gcp"));
        assert_eq!(model.active_provider(), "gcp");
        assert_eq!(model.connection(), None);
        assert!(model.connections().is_empty());
        assert!(model.dimensions().is_empty());
        assert!(model.selection().is_empty());
        assert_eq!(model.connections_state(), &ConnectionsState::Loading);

        assert!(!model.switch_provider("gcp"));
        assert!(!model.switch_provider("missing"));
        assert_eq!(model.active_provider(), "gcp");
    }

    #[test]
    fn labels_and_short_names_come_from_provider_info() {
        let model = ContextModel::new(
            vec![
                entry("aws", "AWS", "Profile", true, true),
                entry("gcp", "Google Cloud", "Project", true, true),
                entry("demo", "Demo", "Profile", false, true),
            ],
            Some("gcp"),
            "aws",
        );
        assert_eq!(model.provider_name(), Some("Google Cloud"));
        assert_eq!(model.connection_label(), Some("Project"));
        let menu = model.provider_menu();
        assert_eq!(
            menu.iter()
                .map(|r| r.short_name.as_str())
                .collect::<Vec<_>>(),
            ["All", "AWS", "GCP", "Demo"]
        );
        assert!(menu[2].active && !menu[0].active && !menu[1].active);

        let aws = ContextModel::new(
            vec![entry("aws", "AWS", "Profile", true, true)],
            None,
            "aws",
        );
        assert_eq!(aws.connection_label(), Some("Profile"));
    }

    #[test]
    fn auth_error_gives_sign_in_state_and_ok_keeps_listed_connection() {
        let mut model = ContextModel::new(
            vec![entry("gcp", "Google Cloud", "Project", true, false)],
            None,
            "gcp",
        );
        model.set_connections(Ok(vec![connection("a")]));
        model.set_dimensions(vec![region_dimension()]);
        model.set_connections(Err(ProviderError::of(ErrorKind::Auth)));
        assert!(model.needs_sign_in());
        assert_eq!(model.connection(), None);
        assert!(model.dimensions().is_empty());
        assert!(model.selection().is_empty());

        model.set_connections(Ok(vec![connection("a"), connection("b")]));
        assert_eq!(model.connection(), Some(&ConnectionId::from("a")));
        assert!(model.select_connection("b"));
        model.set_connections(Ok(vec![connection("b"), connection("c")]));
        assert_eq!(model.connection(), Some(&ConnectionId::from("b")));
        model.set_connections(Ok(vec![connection("c")]));
        assert_eq!(model.connection(), Some(&ConnectionId::from("c")));
        assert_eq!(model.connections_state(), &ConnectionsState::Ready);
    }

    #[test]
    fn other_errors_are_failed_with_message() {
        let mut model = ContextModel::new(
            vec![entry("aws", "AWS", "Profile", true, true)],
            None,
            "aws",
        );
        model.set_connections(Err(ProviderError::new(ErrorKind::Network, "no route")));
        assert_eq!(
            model.connections_state(),
            &ConnectionsState::Failed("no route".to_string())
        );
        assert!(!model.needs_sign_in());
    }

    #[test]
    fn defaults_come_from_default_flags_or_roots() {
        let mut dim = region_dimension();
        dim.values[1].default = true;
        let model = model_with_region(dim);
        assert_eq!(model.selection().values("region"), ids(&["us-west-2"]));

        let all = model_with_region(region_dimension());
        assert_eq!(
            all.selection().values("region"),
            ids(&["us-east-1", "us-west-2", "eu-west-1"])
        );
        assert!(all.is_all("region"));

        let single = model_with_region(dimension(
            "zone",
            "Zone",
            false,
            false,
            vec![value("a", "a", None, false), value("b", "b", None, false)],
        ));
        assert_eq!(single.selection().values("zone"), ids(&["a"]));

        let optional = model_with_region(dimension(
            "place",
            "Place",
            true,
            true,
            vec![value("a", "a", None, false)],
        ));
        assert!(optional.selection().values("place").is_empty());
        assert!(optional.is_all("place"));
    }

    #[test]
    fn selecting_a_parent_checks_children_and_normalizes() {
        let mut model = model_with_region(tree_dimension());
        model.set_values("region", ids(&["us"]));
        assert!(model.is_checked("region", &ValueId::from("us-east-1")));
        assert!(!model.is_checked("region", &ValueId::from("eu")));

        model.set_values("region", ids(&["us", "us-east-1"]));
        assert_eq!(model.selection().values("region"), ids(&["us"]));
    }

    #[test]
    fn lone_child_implies_parent() {
        let mut model = model_with_region(tree_dimension());
        model.set_values("region", ids(&["us-east-1"]));
        assert_eq!(model.implied_parents("region"), ids(&["us"]));
        assert!(!model.is_checked("region", &ValueId::from("us")));
    }

    #[test]
    fn toggling_a_child_under_selected_parent_keeps_siblings() {
        let mut model = model_with_region(tree_dimension());
        model.set_values("region", ids(&["us"]));
        model.toggle("region", &ValueId::from("us-east-1"));
        assert_eq!(model.selection().values("region"), ids(&["us-west-2"]));
        assert!(!model.is_checked("region", &ValueId::from("us-east-1")));
        assert!(model.is_checked("region", &ValueId::from("us-west-2")));
    }

    #[test]
    fn single_select_keeps_one_value() {
        let mut model = model_with_region(dimension(
            "zone",
            "Zone",
            false,
            false,
            vec![value("a", "a", None, false), value("b", "b", None, false)],
        ));
        model.set_values("zone", ids(&["a", "b"]));
        assert_eq!(model.selection().values("zone"), ids(&["b"]));
        model.toggle("zone", &ValueId::from("a"));
        assert_eq!(model.selection().values("zone"), ids(&["a"]));
    }

    #[test]
    fn list_result_counts_value_and_ancestors_and_marks_failures() {
        let mut model = model_with_region(tree_dimension());
        let resource = Instance {
            id: "i-1".into(),
            ..Instance::default()
        };
        let result = ListResult {
            resources: vec![resource.clone(), resource],
            failures: vec![
                CellFailure {
                    cell: Selection::new().with("region", ids(&["us-east-1"])),
                    error: ProviderError::of(ErrorKind::Timeout),
                },
                CellFailure {
                    cell: Selection::new(),
                    error: ProviderError::of(ErrorKind::Timeout),
                },
            ],
        };
        model.set_list_result(&result, |_| ids(&["us-east-1"]));

        assert_eq!(model.value_count("region", &ValueId::from("us-east-1")), 2);
        assert_eq!(model.value_count("region", &ValueId::from("us")), 2);
        assert_eq!(model.value_count("region", &ValueId::from("eu")), 0);
        assert!(model.failed("region", &ValueId::from("us-east-1")));
        assert!(model.failed("region", &ValueId::from("us")));
        assert!(!model.failed("region", &ValueId::from("us-west-2")));
        assert!(model.whole_failure().is_some());
    }

    #[test]
    fn rows_put_all_first_children_after_parent_and_keep_matching_ancestors() {
        let mut model = model_with_region(tree_dimension());
        model.set_values("region", ids(&["us-east-1"]));
        let rows = model.rows("region", "");
        let labels: Vec<(&str, usize)> = rows.iter().map(|r| (r.label.as_str(), r.depth)).collect();
        assert_eq!(
            labels,
            [
                ("All regions", 0),
                ("US", 0),
                ("us-east-1", 1),
                ("us-west-2", 1),
                ("EU", 0)
            ]
        );
        assert!(rows[2].checked);
        assert!(rows[1].implied);

        let filtered = model.rows("region", "east");
        let labels: Vec<&str> = filtered.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, ["US", "us-east-1"]);
    }

    #[test]
    fn summary_matches_region_scope_label_for_region_dimension() {
        let mut model = model_with_region(region_dimension());
        assert_eq!(model.summary("region"), "All regions");
        model.set_values("region", ids(&["us-east-1"]));
        assert_eq!(model.summary("region"), "us-east-1");
        model.set_values("region", ids(&["us-east-1", "us-west-2"]));
        assert_eq!(model.summary("region"), "2 regions");

        let zone = model_with_region(dimension(
            "zone",
            "Zone",
            true,
            false,
            vec![value("a", "a", None, false)],
        ));
        assert_eq!(zone.summary("zone"), "All zones");

        let mut custom = region_dimension();
        custom.plural_label = Some("Areas".to_string());
        let custom = model_with_region(custom);
        assert_eq!(custom.summary("region"), "All areas");
    }

    #[test]
    fn count_text_uses_kind_label() {
        let mut model = ContextModel::new(
            vec![entry("aws", "AWS", "Profile", true, true)],
            None,
            "aws",
        );
        assert_eq!(model.count_text(1), "1 instance");
        assert_eq!(model.count_text(3), "3 instances");
        model.set_kind_label("Databases");
        assert_eq!(model.count_text(1), "1 database");
    }

    #[test]
    fn model_source_has_no_provider_or_region_names_outside_tests() {
        let source = include_str!("context_model.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        let lower = production.to_lowercase();
        assert!(!lower.contains("aws"), "production code names a provider");
        assert!(!lower.contains("region"), "production code names a region");
    }
}

#[cfg(test)]
mod provider_switch_tests {
    use super::*;

    fn entry(id: &str) -> ProviderEntry {
        ProviderEntry {
            info: ProviderInfo {
                id: id.to_string(),
                name: id.to_string(),
                version: "1.0.0".to_string(),
                connection_label: "Project".to_string(),
            },
            signed_in: true,
            bundled: true,
        }
    }

    #[test]
    fn switching_to_another_registered_provider_persists_it() {
        let mut model = ContextModel::new(vec![entry("aws"), entry("gcp")], None, "aws");
        let mut settings = Settings::default();

        assert!(switch_active_provider(&mut model, &mut settings, "gcp"));
        assert_eq!(model.active_provider(), "gcp");
        assert_eq!(settings.active_provider.as_deref(), Some("gcp"));
    }

    #[test]
    fn same_or_unknown_provider_leaves_settings_untouched() {
        let mut model = ContextModel::new(vec![entry("aws"), entry("gcp")], None, "aws");
        let mut settings = Settings::default();

        assert!(!switch_active_provider(&mut model, &mut settings, "aws"));
        assert!(!switch_active_provider(
            &mut model,
            &mut settings,
            "missing"
        ));
        assert_eq!(model.active_provider(), "aws");
        assert_eq!(settings.active_provider, None);
    }

    #[test]
    fn connection_tooltip_names_the_connection_label_and_shortcut() {
        assert_eq!(connection_tooltip("Profile", "⌘P"), "Switch profile  ⌘P");
        assert_eq!(connection_tooltip("Project", "⌘P"), "Switch project  ⌘P");
    }
}
