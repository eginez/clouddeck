mod common;

use clouddeck_extension_api::{AddressKind, ErrorKind, Instance, StateKind};
use clouddeck_gcp::{ProjectsPage, ZonesPage};
use clouddeck_gcp::{
    console_url, map_connections, map_instance, map_state, parse_adc, region_of, zone_dimension,
};
use common::fixture;

fn parse<T: serde::de::DeserializeOwned>(name: &str) -> T {
    serde_json::from_str(&fixture(name)).unwrap()
}

#[test]
fn state_mapping_follows_the_table() {
    use StateKind::*;
    let cases = [
        ("RUNNING", Running),
        ("PROVISIONING", Starting),
        ("STAGING", Starting),
        ("STOPPING", Stopping),
        ("SUSPENDED", Stopped),
        ("SUSPENDING", Stopped),
        ("TERMINATED", Stopped),
        ("REPAIRING", Unknown),
        ("", Unknown),
    ];
    for (raw, kind) in cases {
        assert_eq!(map_state(raw), kind, "{raw}");
    }
}

#[test]
fn region_is_derived_from_zone() {
    assert_eq!(region_of("us-central1-a"), "us-central1");
    assert_eq!(region_of("europe-west1-b"), "europe-west1");
}

#[test]
fn only_active_projects_become_connections() {
    let connections = map_connections(parse::<ProjectsPage>("projects.json").projects);
    let ids: Vec<_> = connections.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["demo-prod", "demo-sandbox"]);
    assert_eq!(connections[0].display_name, "Demo Production");
}

#[test]
fn zones_form_a_region_zone_tree() {
    let dimension = zone_dimension(&parse::<ZonesPage>("zones.json").items);
    assert_eq!(dimension.id.as_str(), "zone");
    let roots: Vec<_> = dimension
        .values
        .iter()
        .filter(|v| v.parent.is_none())
        .collect();
    assert_eq!(roots.len(), 2);
    let child = dimension
        .values
        .iter()
        .find(|v| v.id.as_str() == "europe-west1-c")
        .unwrap();
    assert_eq!(child.parent.as_ref().unwrap().as_str(), "europe-west1");
}

fn web_1() -> Instance {
    let all = parse::<serde_json::Value>("aggregated_instances.json");
    let gce = all["items"]["zones/us-central1-a"]["instances"][0].clone();
    map_instance(&serde_json::from_value(gce).unwrap(), "us-central1-a")
}

#[test]
fn instance_fields_are_mapped() {
    let web = web_1();
    assert_eq!(web.id, "us-central1-a/web-1");
    assert_eq!(web.instance_type.as_deref(), Some("e2-medium"));
    assert_eq!(web.region.as_deref(), Some("us-central1"));
    assert_eq!(web.location.as_deref(), Some("us-central1-a"));
    assert_eq!(web.state.kind, StateKind::Running);
    assert_eq!(web.address(AddressKind::Private), Some("10.128.0.2"));
    assert_eq!(web.address(AddressKind::Public), Some("34.123.10.20"));
    assert_eq!(web.labels.len(), 2);
    assert_eq!(web.labels[0].value.as_deref(), Some("prod"));
    assert!(web.created.is_some());
}

#[test]
fn console_url_has_zone_name_and_project() {
    assert_eq!(
        console_url("demo-prod", "us-central1-a", "web-1"),
        "https://console.cloud.google.com/compute/instancesDetail/zones/us-central1-a/instances/web-1?project=demo-prod"
    );
}

#[test]
fn adc_authorized_user_parses_and_other_types_are_refused() {
    let creds = parse_adc(&fixture("adc_authorized_user.json")).unwrap();
    assert_eq!(creds.quota_project.as_deref(), Some("demo-prod"));
    let error = parse_adc(&fixture("adc_service_account.json")).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Auth);
    assert_eq!(
        error.advice.as_deref(),
        Some("run: gcloud auth application-default login, or choose a Config folder")
    );
}
