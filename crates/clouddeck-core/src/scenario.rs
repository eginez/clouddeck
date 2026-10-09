use std::fs;

use serde::Deserialize;

use crate::models::{EC2Instance, EC2InstanceTag, EC2SecurityGroup};
use crate::service::BoxError;

/// A named EC2 scenario (Swift: `EC2Scenario` in `UITestScenario.swift`).
///
/// Mirrors the vendor fixture shape `{name, description, profiles[].regions[].instances[]}`.
/// Fields the Swift oracle does not carry (region `fault`, `delayMilliseconds`,
/// `failureKind`, `failureMessage`) are ignored here; the fault-injection
/// consumers arrive in their own later work.
#[derive(Debug, Clone, PartialEq)]
pub struct EC2Scenario {
    pub name: String,
    pub description: Option<String>,
    pub profiles: Vec<EC2ScenarioProfile>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EC2ScenarioProfile {
    pub name: String,
    pub regions: Vec<EC2ScenarioRegion>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EC2ScenarioRegion {
    pub name: String,
    pub instances: Vec<EC2Instance>,
}

impl EC2Scenario {
    /// First profile + region pair, if any (Swift: `defaultSelection`).
    pub fn default_selection(&self) -> Option<(String, String)> {
        let profile = self.profiles.first()?;
        let region = profile.regions.first()?;
        Some((profile.name.clone(), region.name.clone()))
    }

    /// Instances for a profile + region (Swift: `instances(profileName:region:)`).
    ///
    /// Fails clearly instead of silently falling back (Swift: same rule).
    pub fn instances(
        &self,
        profile_name: Option<&str>,
        region: &str,
    ) -> Result<Vec<EC2Instance>, BoxError> {
        let profile_name = profile_name
            .filter(|name| !name.is_empty())
            .ok_or_else(|| {
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "No scenario profile selected. Choose a profile to load scenario instances.",
                )) as BoxError
            })?;

        let profile = self
            .profiles
            .iter()
            .find(|profile| profile.name == profile_name)
            .ok_or_else(|| {
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!(
                        "Scenario '{}' does not define AWS profile '{profile_name}'.",
                        self.name
                    ),
                )) as BoxError
            })?;

        let scenario_region = profile
            .regions
            .iter()
            .find(|scenario_region| scenario_region.name == region)
            .ok_or_else(|| {
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!(
                        "Scenario '{}' does not define region '{region}' for profile '{profile_name}'.",
                        self.name
                    ),
                )) as BoxError
            })?;

        Ok(scenario_region.instances.clone())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawScenario {
    name: String,
    description: Option<String>,
    profiles: Vec<RawProfile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProfile {
    name: String,
    regions: Vec<RawRegion>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRegion {
    name: String,
    #[serde(default)]
    instances: Vec<RawInstance>,
}

fn parse_launch_time(raw: Option<&str>) -> Option<std::time::SystemTime> {
    let raw = raw?;
    chrono::DateTime::parse_from_rfc3339(raw)
        .ok()
        .and_then(|parsed| {
            let timestamp = parsed.timestamp();
            let nanos = parsed.timestamp_subsec_nanos();
            if timestamp < 0 {
                return None;
            }
            let duration = std::time::Duration::from_secs(timestamp as u64)
                .checked_add(std::time::Duration::from_nanos(nanos as u64))?;
            Some(std::time::UNIX_EPOCH + duration)
        })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawInstance {
    #[serde(rename = "instanceID")]
    instance_id: String,
    #[serde(rename = "instanceType")]
    instance_type: Option<String>,
    state: Option<String>,
    #[serde(rename = "launchTime")]
    launch_time: Option<String>,
    #[serde(rename = "publicIPAddress")]
    public_ip_address: Option<String>,
    #[serde(rename = "privateIPAddress")]
    private_ip_address: Option<String>,
    #[serde(rename = "vpcID")]
    vpc_id: Option<String>,
    #[serde(rename = "subnetID")]
    subnet_id: Option<String>,
    #[serde(rename = "securityGroups")]
    security_groups: Option<Vec<RawSecurityGroup>>,
    tags: Option<Vec<RawTag>>,
    #[serde(rename = "availabilityZone")]
    availability_zone: Option<String>,
    #[serde(rename = "imageID")]
    image_id: Option<String>,
    #[serde(rename = "keyName")]
    key_name: Option<String>,
    architecture: Option<String>,
    #[serde(rename = "stateTransitionReason")]
    state_transition_reason: Option<String>,
    #[serde(rename = "iamInstanceProfileARN")]
    iam_instance_profile_arn: Option<String>,
    #[serde(rename = "privateDNSName")]
    private_dns_name: Option<String>,
    #[serde(rename = "publicDNSName")]
    public_dns_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSecurityGroup {
    #[serde(rename = "groupID")]
    group_id: Option<String>,
    #[serde(rename = "groupName")]
    group_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTag {
    key: String,
    value: String,
}

/// A catalog that resolves scenario names to parsed scenarios (Swift:
/// `EC2ScenarioCatalog`).
pub trait EC2ScenarioCatalog: Send + Sync {
    fn scenario(&self, name: &str) -> Result<EC2Scenario, BoxError>;
    fn scenario_names(&self) -> Vec<String>;
}

/// Catalog over a directory of `<name>.json` scenario files (Swift:
/// `BundleScenarioCatalog`, directory fallback).
#[derive(Debug, Clone, Default)]
pub struct DirectoryScenarioCatalog {
    root: Option<std::path::PathBuf>,
}

impl DirectoryScenarioCatalog {
    pub fn new(root: impl Into<std::path::PathBuf>) -> Self {
        Self {
            root: Some(root.into()),
        }
    }

    /// Loads every `<name>.json` in `root`, keyed by the in-file `name`
    /// (which matches the file stem by convention).
    pub fn load_all(root: impl Into<std::path::PathBuf>) -> Result<Vec<EC2Scenario>, BoxError> {
        let root = root.into();
        let mut scenarios = Vec::new();
        for entry in fs::read_dir(&root).map_err(|err| {
            std::io::Error::new(
                err.kind(),
                format!(
                    "Could not list scenario directory '{}': {err}",
                    root.display()
                ),
            )
        })? {
            let entry = entry?;
            let file_name = entry.file_name();
            let file_name = file_name.to_string_lossy();
            let is_json = file_name.ends_with(".json");
            if !is_json {
                continue;
            }

            let path = entry.path();
            let contents = fs::read_to_string(&path).map_err(|err| {
                std::io::Error::new(
                    err.kind(),
                    format!("Could not read scenario '{}': {err}", path.display()),
                )
            })?;
            let scenario = parse_scenario(&contents)?;
            scenarios.push(scenario);
        }

        scenarios.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(scenarios)
    }
}

impl EC2ScenarioCatalog for DirectoryScenarioCatalog {
    fn scenario(&self, name: &str) -> Result<EC2Scenario, BoxError> {
        let Some(root) = self.root.as_ref() else {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Scenario catalog is unavailable.",
            )));
        };

        let path = root.join(format!("{name}.json"));
        if !path.exists() {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Scenario '{name}' is unavailable."),
            )));
        }

        let contents = fs::read_to_string(&path).map_err(|err| {
            std::io::Error::new(
                err.kind(),
                format!("Could not read scenario '{name}': {err}"),
            )
        })?;

        parse_scenario(&contents)
    }

    fn scenario_names(&self) -> Vec<String> {
        let Some(root) = self.root.as_ref() else {
            return Vec::new();
        };

        let Ok(entries) = fs::read_dir(root) else {
            return Vec::new();
        };

        entries
            .flatten()
            .filter_map(|entry| {
                let file_name = entry.file_name().to_string_lossy().to_string();
                file_name.strip_suffix(".json").map(str::to_string)
            })
            .collect()
    }
}

/// Parses scenario JSON into an [`EC2Scenario`], mapping instances into
/// [`EC2Instance`] with the enclosing region's name set.
///
/// Mirrors the Swift oracle's decode rules:
/// - blank strings decode as `None`
/// - a `Name` tag with a blank value is dropped
/// - ISO-8601 `launchTime` parses to `SystemTime`
pub fn parse_scenario(contents: &str) -> Result<EC2Scenario, BoxError> {
    let raw: RawScenario = serde_json::from_str(contents).map_err(|err| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Invalid scenario JSON: {err}"),
        )
    })?;

    Ok(EC2Scenario {
        name: raw.name,
        description: raw.description,
        profiles: raw
            .profiles
            .into_iter()
            .map(|profile| EC2ScenarioProfile {
                name: profile.name,
                regions: profile
                    .regions
                    .into_iter()
                    .map(|region| {
                        let region_name = region.name.clone();
                        EC2ScenarioRegion {
                            name: region.name,
                            instances: region
                                .instances
                                .into_iter()
                                .map(|instance| map_instance(instance, &region_name))
                                .collect(),
                        }
                    })
                    .collect(),
            })
            .collect(),
    })
}

fn map_instance(raw: RawInstance, region: &str) -> EC2Instance {
    let blankable = |value: Option<String>| -> Option<String> {
        value.and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(value)
            }
        })
    };

    let tags = raw
        .tags
        .unwrap_or_default()
        .into_iter()
        .filter(|tag| !(tag.key.eq_ignore_ascii_case("Name") && tag.value.trim().is_empty()))
        .map(|tag| EC2InstanceTag {
            key: tag.key,
            value: tag.value,
        })
        .collect();

    let launch_time = parse_launch_time(raw.launch_time.as_deref());

    EC2Instance {
        instance_id: raw.instance_id,
        region: Some(region.to_string()),
        instance_type: blankable(raw.instance_type),
        state: blankable(raw.state),
        launch_time,
        public_ip_address: blankable(raw.public_ip_address),
        private_ip_address: blankable(raw.private_ip_address),
        vpc_id: blankable(raw.vpc_id),
        subnet_id: blankable(raw.subnet_id),
        security_groups: raw
            .security_groups
            .unwrap_or_default()
            .into_iter()
            .map(|group| EC2SecurityGroup {
                group_id: group.group_id,
                group_name: group.group_name,
            })
            .collect(),
        tags,
        availability_zone: blankable(raw.availability_zone),
        image_id: blankable(raw.image_id),
        key_name: blankable(raw.key_name),
        architecture: blankable(raw.architecture),
        state_transition_reason: blankable(raw.state_transition_reason),
        iam_instance_profile_arn: blankable(raw.iam_instance_profile_arn),
        private_dns_name: blankable(raw.private_dns_name),
        public_dns_name: blankable(raw.public_dns_name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/ec2-scenarios");

    fn fixture_path(name: &str) -> std::path::PathBuf {
        std::path::Path::new(FIXTURES).join(format!("{name}.json"))
    }

    #[test]
    fn small_happy_path_fixture_is_vendored_verbatim() {
        let vendored = fixture_path("small-happy-path");
        let contents = fs::read_to_string(&vendored).expect("vendored fixture readable");
        let scenario = parse_scenario(&contents).unwrap();
        assert_eq!(scenario.name, "small-happy-path");
        assert_eq!(
            scenario.description.as_deref(),
            Some("Small realistic EC2 fleet with no failures.")
        );
        assert_eq!(scenario.profiles.len(), 1);
        assert_eq!(scenario.profiles[0].name, "scenario-default");
        assert_eq!(scenario.profiles[0].regions.len(), 1);
        assert_eq!(scenario.profiles[0].regions[0].name, "us-east-1");
        assert_eq!(scenario.profiles[0].regions[0].instances.len(), 3);
    }

    #[test]
    fn empty_fleet_fixture_is_vendored_verbatim() {
        let vendored = fixture_path("empty-fleet");
        let contents = fs::read_to_string(&vendored).expect("vendored fixture readable");
        let scenario = parse_scenario(&contents).unwrap();
        assert_eq!(scenario.name, "empty-fleet");
        assert_eq!(scenario.profiles.len(), 1);
        assert_eq!(scenario.profiles[0].regions[0].name, "us-east-1");
        assert!(scenario.profiles[0].regions[0].instances.is_empty());
    }

    #[test]
    fn small_happy_path_loader_returns_parsed_instances_for_both() {
        let contents = fs::read_to_string(fixture_path("small-happy-path")).unwrap();
        let scenario = parse_scenario(&contents).unwrap();

        let instances = scenario
            .instances(Some("scenario-default"), "us-east-1")
            .unwrap();

        assert_eq!(instances.len(), 3);
        assert_eq!(
            instances
                .iter()
                .map(|instance| instance.instance_id.as_str())
                .collect::<Vec<_>>(),
            vec!["i-scenario-001", "i-scenario-002", "i-scenario-003"]
        );

        let first = &instances[0];
        assert_eq!(first.instance_type.as_deref(), Some("t3.medium"));
        assert_eq!(first.state.as_deref(), Some("running"));
        assert_eq!(first.private_ip_address.as_deref(), Some("10.0.1.12"));
        assert_eq!(first.public_ip_address, None);
        assert_eq!(first.vpc_id.as_deref(), Some("vpc-main"));
        assert_eq!(first.subnet_id.as_deref(), Some("subnet-app-a"));
        assert_eq!(first.region.as_deref(), Some("us-east-1"));
        assert_eq!(first.security_groups.len(), 1);
        assert_eq!(first.security_groups[0].group_id.as_deref(), Some("sg-web"));
        assert_eq!(first.security_groups[0].group_name.as_deref(), Some("web"));
        assert_eq!(first.tags.len(), 3);
        assert_eq!(first.name(), Some("prod-api-1"));
        assert!(first.launch_time.is_some());
        // Fields the vendored fixture predates must parse as absent, not fail.
        assert_eq!(first.availability_zone, None);
        assert_eq!(first.image_id, None);
        assert_eq!(first.key_name, None);
        assert_eq!(first.architecture, None);
        assert_eq!(first.state_transition_reason, None);
        assert_eq!(first.iam_instance_profile_arn, None);
        assert_eq!(first.private_dns_name, None);
        assert_eq!(first.public_dns_name, None);

        let second = &instances[1];
        assert_eq!(second.instance_type.as_deref(), Some("m6i.large"));
        assert_eq!(second.state.as_deref(), Some("stopped"));

        let third = &instances[2];
        assert_eq!(third.public_ip_address.as_deref(), Some("203.0.113.22"));
        assert_eq!(third.name(), Some("bastion-east-1"));
    }

    #[test]
    fn empty_fleet_loader_returns_empty_instance_list() {
        let contents = fs::read_to_string(fixture_path("empty-fleet")).unwrap();
        let scenario = parse_scenario(&contents).unwrap();
        let instances = scenario
            .instances(Some("scenario-default"), "us-east-1")
            .unwrap();
        assert!(instances.is_empty());
    }

    #[test]
    fn launch_time_parses_iso8601() {
        let contents = r#"{
            "name": "t",
            "profiles": [{
                "name": "p",
                "regions": [{
                    "name": "us-east-1",
                    "instances": [{
                        "instanceID": "i-1",
                        "launchTime": "2026-04-20T12:00:00Z"
                    }]
                }]
            }]
        }"#;
        let scenario = parse_scenario(contents).unwrap();
        let instance = &scenario.profiles[0].regions[0].instances[0];
        let launch = instance.launch_time.unwrap();
        let unix = launch
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        // 2026-04-20T12:00:00Z == 1776686400.
        assert_eq!(unix, 1776686400);
    }

    #[test]
    fn blank_strings_decode_as_none() {
        let contents = r#"{
            "name": "t",
            "profiles": [{
                "name": "p",
                "regions": [{
                    "name": "us-east-1",
                    "instances": [{
                        "instanceID": "i-1",
                        "instanceType": "  ",
                        "state": "",
                        "publicIPAddress": null,
                        "tags": [{"key": "Name", "value": "   "}]
                    }]
                }]
            }]
        }"#;
        let scenario = parse_scenario(contents).unwrap();
        let instance = &scenario.profiles[0].regions[0].instances[0];
        assert_eq!(instance.instance_type, None);
        assert_eq!(instance.state, None);
        assert_eq!(instance.public_ip_address, None);
        assert!(instance.tags.is_empty());
        assert_eq!(instance.name(), None);
    }

    #[test]
    fn inspector_fields_parse_from_scenario_json() {
        let contents = r#"{
            "name": "t",
            "profiles": [{
                "name": "p",
                "regions": [{
                    "name": "us-east-1",
                    "instances": [{
                        "instanceID": "i-1",
                        "availabilityZone": "us-east-1a",
                        "imageID": "ami-0abc",
                        "keyName": "deploy",
                        "architecture": "x86_64",
                        "stateTransitionReason": "Client.StopInstance User requested stop",
                        "iamInstanceProfileARN": "arn:aws:iam::123:instance-profile/api",
                        "privateDNSName": "ip-10-0-0-1.ec2.internal",
                        "publicDNSName": "ec2-1-2-3-4.us-east-1.compute.amazonaws.com"
                    }]
                }]
            }]
        }"#;
        let scenario = parse_scenario(contents).unwrap();
        let instance = &scenario.profiles[0].regions[0].instances[0];
        assert_eq!(instance.availability_zone.as_deref(), Some("us-east-1a"));
        assert_eq!(instance.image_id.as_deref(), Some("ami-0abc"));
        assert_eq!(instance.key_name.as_deref(), Some("deploy"));
        assert_eq!(instance.architecture.as_deref(), Some("x86_64"));
        assert_eq!(
            instance.state_transition_reason.as_deref(),
            Some("Client.StopInstance User requested stop")
        );
        assert_eq!(
            instance.iam_instance_profile_arn.as_deref(),
            Some("arn:aws:iam::123:instance-profile/api")
        );
        assert_eq!(
            instance.private_dns_name.as_deref(),
            Some("ip-10-0-0-1.ec2.internal")
        );
        assert_eq!(
            instance.public_dns_name.as_deref(),
            Some("ec2-1-2-3-4.us-east-1.compute.amazonaws.com")
        );
    }

    #[test]
    fn region_lookup_fails_clearly_for_unknown_region() {
        let contents = fs::read_to_string(fixture_path("empty-fleet")).unwrap();
        let scenario = parse_scenario(&contents).unwrap();
        let err = scenario
            .instances(Some("scenario-default"), "eu-west-1")
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("does not define region 'eu-west-1'")
        );
    }

    #[test]
    fn region_lookup_fails_clearly_for_unknown_profile() {
        let contents = fs::read_to_string(fixture_path("empty-fleet")).unwrap();
        let scenario = parse_scenario(&contents).unwrap();
        let err = scenario.instances(Some("nope"), "us-east-1").unwrap_err();
        assert!(
            err.to_string()
                .contains("does not define AWS profile 'nope'")
        );
    }

    #[test]
    fn region_lookup_fails_clearly_for_missing_profile() {
        let contents = fs::read_to_string(fixture_path("empty-fleet")).unwrap();
        let scenario = parse_scenario(&contents).unwrap();
        assert!(scenario.instances(None, "us-east-1").is_err());
        assert!(scenario.instances(Some(""), "us-east-1").is_err());
    }

    #[test]
    fn default_selection_uses_first_profile_and_region() {
        let contents = fs::read_to_string(fixture_path("small-happy-path")).unwrap();
        let scenario = parse_scenario(&contents).unwrap();
        assert_eq!(
            scenario.default_selection(),
            Some(("scenario-default".to_string(), "us-east-1".to_string()))
        );
    }

    #[test]
    fn catalog_loads_named_scenario_from_directory() {
        let catalog = DirectoryScenarioCatalog::new(FIXTURES);
        let scenario = catalog.scenario("small-happy-path").unwrap();
        assert_eq!(scenario.name, "small-happy-path");

        let empty = catalog.scenario("empty-fleet").unwrap();
        assert!(empty.profiles[0].regions[0].instances.is_empty());
    }

    #[test]
    fn catalog_lists_scenario_names() {
        let catalog = DirectoryScenarioCatalog::new(FIXTURES);
        let names = catalog.scenario_names();
        assert!(names.contains(&"small-happy-path".to_string()));
        assert!(names.contains(&"empty-fleet".to_string()));
    }

    #[test]
    fn catalog_reports_missing_scenario() {
        let catalog = DirectoryScenarioCatalog::new(FIXTURES);
        let err = catalog.scenario("nope").unwrap_err();
        assert!(err.to_string().contains("Scenario 'nope' is unavailable."));
    }

    #[test]
    fn loader_is_not_hardcoded_to_the_two_vendored_names() {
        let contents = r#"{
            "name": "some-other-scenario",
            "description": "arbitrary",
            "profiles": [{
                "name": "prod",
                "regions": [{
                    "name": "eu-central-1",
                    "instances": [{
                        "instanceID": "i-x",
                        "instanceType": "c7a.xlarge",
                        "state": "running",
                        "tags": [{"key": "Name", "value": "x"}]
                    }]
                }]
            }]
        }"#;
        let scenario = parse_scenario(contents).unwrap();
        let instances = scenario.instances(Some("prod"), "eu-central-1").unwrap();
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].instance_id, "i-x");
        assert_eq!(instances[0].region.as_deref(), Some("eu-central-1"));
    }
}
