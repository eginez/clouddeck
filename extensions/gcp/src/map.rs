//! Pure mapping from Google API JSON shapes to CloudDeck types. No IO here.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, SystemTime};

use clouddeck_extension_api::{
    Address, AddressKind, CellFailure, Connection, DimValue, Dimension, ErrorKind, ExtrasSection,
    Instance, Label, ProviderError, Selection, State, StateKind, ValueId,
};
use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

pub const ZONE_DIM: &str = "zone";
/// Benign: Compute reports it for every scope that simply has no instances.
const EMPTY_SCOPE: &str = "NO_RESULTS_ON_PAGE";

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProjectsPage {
    #[serde(default)]
    pub projects: Vec<Project>,
    pub next_page_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub project_id: String,
    pub name: Option<String>,
    pub lifecycle_state: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ZonesPage {
    #[serde(default)]
    pub items: Vec<Zone>,
    pub next_page_token: Option<String>,
}

#[derive(Deserialize)]
pub struct Zone {
    pub name: String,
    pub region: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AggregatedPage {
    #[serde(default)]
    pub items: BTreeMap<String, Scope>,
    pub next_page_token: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct Scope {
    #[serde(default)]
    pub instances: Vec<GceInstance>,
    pub warning: Option<Warning>,
}

#[derive(Deserialize)]
pub struct Warning {
    pub code: Option<String>,
    pub message: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GceInstance {
    pub name: String,
    pub status: Option<String>,
    pub status_message: Option<String>,
    pub machine_type: Option<String>,
    pub cpu_platform: Option<String>,
    pub creation_timestamp: Option<String>,
    pub zone: Option<String>,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    #[serde(default)]
    pub network_interfaces: Vec<NetworkInterface>,
    pub deletion_protection: Option<bool>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NetworkInterface {
    #[serde(rename = "networkIP")]
    pub network_ip: Option<String>,
    pub network: Option<String>,
    pub subnetwork: Option<String>,
    #[serde(default)]
    pub access_configs: Vec<AccessConfig>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AccessConfig {
    #[serde(rename = "natIP")]
    pub nat_ip: Option<String>,
}

pub fn last_segment(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// `us-central1-a` -> `us-central1`.
pub fn region_of(zone: &str) -> String {
    zone.rsplit_once('-')
        .map_or(zone, |(region, _)| region)
        .to_string()
}

pub fn map_connections(projects: Vec<Project>) -> Vec<Connection> {
    projects
        .into_iter()
        .filter(|p| p.lifecycle_state.as_deref().is_none_or(|s| s == "ACTIVE"))
        .map(|p| Connection {
            display_name: p.name.unwrap_or_else(|| p.project_id.clone()),
            id: p.project_id.into(),
        })
        .collect()
}

/// One tree under one dimension id: regions are roots, their zones are children.
pub fn zone_dimension(zones: &[Zone]) -> Dimension {
    let mut values = Vec::new();
    let mut regions = BTreeSet::new();
    for zone in zones {
        let region = zone_region(zone);
        if regions.insert(region.clone()) {
            values.push(dim_value(&region, None));
        }
        values.push(dim_value(&zone.name, Some(region)));
    }
    Dimension {
        id: ZONE_DIM.into(),
        label: "Zone".to_string(),
        plural_label: None,
        multi: true,
        optional: true,
        values,
    }
}

fn zone_region(zone: &Zone) -> String {
    match &zone.region {
        Some(url) => last_segment(url).to_string(),
        None => region_of(&zone.name),
    }
}

fn dim_value(id: &str, parent: Option<String>) -> DimValue {
    DimValue {
        id: id.into(),
        label: id.to_string(),
        parent: parent.map(ValueId::from),
        default: false,
    }
}

/// Expands selected regions/zones into zone names; returns the values nothing matched.
pub fn expand_selection(
    dimension: &Dimension,
    selected: &[ValueId],
) -> (BTreeSet<String>, Vec<ValueId>) {
    let mut zones = BTreeSet::new();
    let mut unknown = Vec::new();
    for id in selected {
        let matched: Vec<&DimValue> = dimension
            .values
            .iter()
            .filter(|v| &v.id == id || v.parent.as_ref() == Some(id))
            .collect();
        if matched.is_empty() {
            unknown.push(id.clone());
        }
        zones.extend(
            matched
                .iter()
                .filter(|v| v.parent.is_some())
                .map(|v| v.id.to_string()),
        );
    }
    (zones, unknown)
}

pub fn failure_for(zone: &str, error: ProviderError) -> CellFailure {
    CellFailure {
        cell: Selection::new().with(ZONE_DIM, vec![ValueId::from(zone)]),
        error,
    }
}

/// Splits an aggregated scope map into instances and per-zone failures.
pub fn map_scopes(
    scopes: BTreeMap<String, Scope>,
    wanted: Option<&BTreeSet<String>>,
) -> (Vec<Instance>, Vec<CellFailure>) {
    let mut resources = Vec::new();
    let mut failures = Vec::new();
    for (scope, body) in scopes {
        let zone = scope.strip_prefix("zones/").unwrap_or(&scope).to_string();
        if wanted.is_some_and(|w| !w.contains(&zone)) {
            continue;
        }
        failures.extend(scope_failure(&zone, body.warning.as_ref()));
        resources.extend(body.instances.iter().map(|i| map_instance(i, &zone)));
    }
    (resources, failures)
}

fn scope_failure(zone: &str, warning: Option<&Warning>) -> Option<CellFailure> {
    let warning = warning.filter(|w| w.code.as_deref() != Some(EMPTY_SCOPE))?;
    let code = warning.code.as_deref().unwrap_or("WARNING");
    let text = warning
        .message
        .as_deref()
        .unwrap_or("scope reported a warning");
    let message = format!("{zone}: {code}: {text}");
    Some(failure_for(
        zone,
        ProviderError::new(ErrorKind::Other, message),
    ))
}

pub fn map_state(status: &str) -> StateKind {
    match status {
        "RUNNING" => StateKind::Running,
        "PROVISIONING" | "STAGING" => StateKind::Starting,
        "STOPPING" => StateKind::Stopping,
        "SUSPENDED" | "TERMINATED" | "SUSPENDING" => StateKind::Stopped,
        _ => StateKind::Unknown,
    }
}

pub fn parse_timestamp(text: &str) -> Option<SystemTime> {
    let secs = OffsetDateTime::parse(text, &Rfc3339).ok()?.unix_timestamp();
    let secs = u64::try_from(secs).ok()?;
    Some(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
}

fn map_addresses(interfaces: &[NetworkInterface]) -> Vec<Address> {
    let address = |kind, value: &String| Address {
        kind,
        value: value.clone(),
    };
    let mut out = Vec::new();
    for nic in interfaces {
        out.extend(
            nic.network_ip
                .iter()
                .map(|ip| address(AddressKind::Private, ip)),
        );
        let nat = nic.access_configs.iter().filter_map(|a| a.nat_ip.as_ref());
        out.extend(nat.map(|ip| address(AddressKind::Public, ip)));
    }
    out
}

fn map_labels(labels: &BTreeMap<String, String>) -> Vec<Label> {
    labels
        .iter()
        .map(|(k, v)| Label {
            key: k.clone(),
            value: Some(v.clone()),
        })
        .collect()
}

/// `zone` is the scope (or request) zone, so list and describe produce the same `zone/name` id.
pub fn map_instance(gce: &GceInstance, zone: &str) -> Instance {
    let raw = gce.status.clone().unwrap_or_default();
    Instance {
        id: format!("{zone}/{}", gce.name),
        name: Some(gce.name.clone()),
        state: State {
            kind: map_state(&raw),
            raw,
        },
        state_reason: gce.status_message.clone(),
        instance_type: gce
            .machine_type
            .as_deref()
            .map(|m| last_segment(m).to_string()),
        region: Some(region_of(zone)),
        location: Some(zone.to_string()),
        created: gce.creation_timestamp.as_deref().and_then(parse_timestamp),
        addresses: map_addresses(&gce.network_interfaces),
        labels: map_labels(&gce.labels),
        extras: instance_extras(gce),
        ..Default::default()
    }
}

fn instance_extras(gce: &GceInstance) -> Vec<ExtrasSection> {
    let entry = |k: &str, v: Option<&str>| (k.to_string(), v.unwrap_or_default().to_string());
    let protection = gce.deletion_protection.map(|b| b.to_string());
    let machine = ExtrasSection {
        title: "Machine".to_string(),
        entries: vec![
            entry("CPU platform", gce.cpu_platform.as_deref()),
            entry("Deletion protection", protection.as_deref()),
        ],
    };
    let networks = gce.network_interfaces.iter().flat_map(|nic| {
        [
            entry("Network", nic.network.as_deref().map(last_segment)),
            entry("Subnetwork", nic.subnetwork.as_deref().map(last_segment)),
        ]
    });
    let network = ExtrasSection {
        title: "Network".to_string(),
        entries: networks.collect(),
    };
    vec![machine, network]
}

pub fn console_url(project: &str, zone: &str, name: &str) -> String {
    format!(
        "https://console.cloud.google.com/compute/instancesDetail/zones/{zone}/instances/{name}?project={project}"
    )
}

/// Splits `zone/name`; `None` for ids that are not parent-qualified.
pub fn split_id(id: &str) -> Option<(&str, &str)> {
    id.split_once('/')
        .filter(|(z, n)| !z.is_empty() && !n.is_empty())
}
