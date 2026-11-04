use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clouddeck_extension_api::conformance::check_provider;
use clouddeck_extension_api::wire::{Method, dispatch};
use clouddeck_extension_api::{
    AuthAnswer, AuthInput, AuthMethod, AuthStatus, AuthStep, Call, CellFailure, Connection,
    ConnectionId, CoreField, DimValue, Dimension, ErrorKind, Extension, ExtrasSection, FieldDef,
    FieldKind, FieldSource, FlowId, Host, INSTANCES, Instance, KindId, KindInfo, ListResult,
    LogLevel, MatchMode, PromptField, Provider, ProviderError, ProviderInfo, ResourceId, Selection,
    ValueId,
};

const REGION_TREE: [(&str, Option<&str>); 3] =
    [("us", None), ("us-east", Some("us")), ("eu", None)];

#[derive(Default)]
struct Fake {
    duplicate_ids: bool,
    panic_on_describe: bool,
    signed_out: bool,
    ignores_cancel: bool,
    hangs_on_cancel: bool,
    flat_regions: bool,
    parent_cycle: bool,
    failure_outside_selection: bool,
}

fn region_parent(region: &str) -> Option<&'static str> {
    REGION_TREE
        .iter()
        .find(|(id, _)| *id == region)
        .and_then(|(_, parent)| *parent)
}

fn selects(selected: &str, region: &str, flat: bool) -> bool {
    if flat {
        return selected == region;
    }
    let mut current = Some(region);
    while let Some(id) = current {
        if id == selected {
            return true;
        }
        current = region_parent(id);
    }
    false
}

fn instances() -> Vec<Instance> {
    [("us-east", "web-1"), ("eu", "web-2")]
        .into_iter()
        .map(|(region, name)| Instance {
            id: format!("{region}/{name}"),
            name: Some(name.into()),
            region: Some(region.into()),
            extras: vec![ExtrasSection {
                title: "Details".into(),
                entries: vec![("AMI".into(), "ami-1".into())],
            }],
            ..Default::default()
        })
        .collect()
}

fn value(id: &str, parent: Option<&str>) -> DimValue {
    DimValue {
        id: id.into(),
        label: id.into(),
        parent: parent.map(Into::into),
        default: false,
    }
}

fn field(name: &str, source: FieldSource) -> FieldDef {
    FieldDef {
        name: name.into(),
        label: name.into(),
        hint: String::new(),
        kind: FieldKind::Text,
        match_mode: MatchMode::Contains,
        query: true,
        search: true,
        facet: false,
        column: true,
        source,
    }
}

impl Provider for Fake {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "fake".into(),
            name: "Fake".into(),
            version: "0.0.0".into(),
            connection_label: "Profile".into(),
        }
    }

    fn auth_status(&self, _: &Call<'_>, _: Option<&ConnectionId>) -> AuthStatus {
        if self.signed_out {
            AuthStatus::SignedOut
        } else {
            AuthStatus::SignedIn { expires: None }
        }
    }

    fn connections(&self, _: &Call<'_>) -> Result<Vec<Connection>, ProviderError> {
        if self.signed_out {
            return Err(ProviderError::of(ErrorKind::Auth));
        }
        Ok(vec![Connection {
            id: "default".into(),
            display_name: "Default".into(),
        }])
    }

    fn dimensions(&self, _: &Call<'_>, _: &ConnectionId) -> Result<Vec<Dimension>, ProviderError> {
        let mut tree: Vec<DimValue> = REGION_TREE
            .iter()
            .map(|(id, parent)| value(id, *parent))
            .collect();
        if self.parent_cycle {
            tree[0].parent = Some("us-east".into());
        }
        Ok(vec![Dimension {
            id: "region".into(),
            label: "Region".into(),
            multi: true,
            optional: true,
            values: tree,
        }])
    }

    fn kinds(&self) -> Vec<KindInfo> {
        vec![KindInfo {
            id: KindId::instances(),
            label: "Instances".into(),
        }]
    }

    fn list(&self, cx: &Call<'_>, _: &ConnectionId, kind: &KindId, sel: &Selection) -> ListResult {
        if self.hangs_on_cancel && cx.host.cancelled() {
            loop {
                std::thread::park();
            }
        }
        if self.ignores_cancel && cx.host.cancelled() {
            std::thread::sleep(Duration::from_secs(6));
        }
        if kind.as_str() != INSTANCES {
            return ListResult::default();
        }
        let regions = sel.values("region");
        let unknown: Vec<ValueId> = regions
            .iter()
            .filter(|r| !REGION_TREE.iter().any(|(id, _)| *id == r.as_str()))
            .cloned()
            .collect();
        let mut failures = match unknown.is_empty() {
            true => vec![],
            false => vec![CellFailure {
                cell: Selection::new().with("region", unknown),
                error: ProviderError::of(ErrorKind::NotFound),
            }],
        };
        if self.failure_outside_selection {
            failures.push(CellFailure {
                cell: Selection::new().with("region", vec!["eu".into()]),
                error: ProviderError::of(ErrorKind::Other),
            });
        }
        let mut resources: Vec<Instance> = instances()
            .into_iter()
            .filter(|i| {
                regions.is_empty()
                    || regions.iter().any(|r| {
                        selects(
                            r.as_str(),
                            i.region.as_deref().unwrap_or_default(),
                            self.flat_regions,
                        )
                    })
            })
            .collect();
        if self.duplicate_ids {
            resources.extend(resources.clone());
        }
        ListResult {
            resources,
            failures,
        }
    }

    fn describe(
        &self,
        _: &Call<'_>,
        _: &ConnectionId,
        _: &KindId,
        id: &ResourceId,
    ) -> Result<Vec<ExtrasSection>, ProviderError> {
        if self.panic_on_describe {
            panic!("describe blew up");
        }
        instances()
            .into_iter()
            .find(|i| i.id == id.as_str())
            .map(|i| i.extras)
            .ok_or_else(|| ProviderError::of(ErrorKind::NotFound))
    }

    fn console_url(&self, c: &ConnectionId, k: &KindId, r: &Instance) -> Option<String> {
        Some(format!("https://console.example/{c}/{k}/{}", r.id))
    }

    fn fields(&self, kind: &KindId) -> Vec<FieldDef> {
        match kind.as_str() == INSTANCES {
            true => vec![
                field("name", FieldSource::Core(CoreField::Name)),
                field(
                    "ami",
                    FieldSource::Extra {
                        section: "Details".to_string(),
                        key: "AMI".to_string(),
                    },
                ),
            ],
            false => vec![],
        }
    }

    fn auth_methods(&self) -> Vec<AuthMethod> {
        vec![AuthMethod {
            id: "token".into(),
            label: "API token".into(),
        }]
    }

    fn begin_auth(
        &self,
        _: &Call<'_>,
        _: &AuthMethod,
        _: AuthInput,
    ) -> Result<AuthStep, ProviderError> {
        Ok(AuthStep::Prompt {
            flow: FlowId::from("flow-1"),
            fields: vec![PromptField {
                id: "token".into(),
                label: "Token".into(),
                secret: true,
            }],
        })
    }

    fn continue_auth(
        &self,
        _: &Call<'_>,
        flow: &FlowId,
        answer: AuthAnswer,
    ) -> Result<AuthStep, ProviderError> {
        match (flow.as_str(), answer) {
            ("flow-1", AuthAnswer::Values(v)) if v.contains_key("token") => Ok(AuthStep::Done),
            ("flow-1", _) => Err(ProviderError::new(ErrorKind::Auth, "token required")),
            _ => Err(ProviderError::new(ErrorKind::Other, "unknown sign-in flow")),
        }
    }
}

#[test]
fn fake_provider_conforms_to_contract_v1() {
    let violations = check_provider(Arc::new(Fake::default()));
    assert!(
        violations.is_empty(),
        "contract v1 violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn signed_out_fake_with_auth_error_conforms() {
    let fake = Fake {
        signed_out: true,
        ..Fake::default()
    };
    let violations = check_provider(Arc::new(fake));
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn checker_reports_duplicate_resource_ids() {
    let fake = Fake {
        duplicate_ids: true,
        ..Fake::default()
    };
    let violations = check_provider(Arc::new(fake));
    assert!(
        violations.iter().any(|v| v.contains("duplicate")),
        "{violations:?}"
    );
}

#[test]
fn checker_reports_list_that_ignores_cancellation() {
    let fake = Fake {
        ignores_cancel: true,
        ..Fake::default()
    };
    let violations = check_provider(Arc::new(fake));
    assert!(
        violations
            .iter()
            .any(|v| v.contains("after the host cancelled")),
        "{violations:?}"
    );
}

#[test]
fn checker_reports_list_that_hangs_after_cancellation() {
    let fake = Fake {
        hangs_on_cancel: true,
        ..Fake::default()
    };
    let violations = check_provider(Arc::new(fake));
    assert!(
        violations
            .iter()
            .any(|v| v.contains("did not return") && v.contains("after the host cancelled")),
        "{violations:?}"
    );
}

#[test]
fn checker_reports_flat_parent_selection() {
    let fake = Fake {
        flat_regions: true,
        ..Fake::default()
    };
    let violations = check_provider(Arc::new(fake));
    assert!(
        violations
            .iter()
            .any(|v| v.contains("does not list exactly its children's resources")),
        "{violations:?}"
    );
}

#[test]
fn checker_reports_parent_cycle() {
    let fake = Fake {
        parent_cycle: true,
        ..Fake::default()
    };
    let violations = check_provider(Arc::new(fake));
    assert!(
        violations.iter().any(|v| v.contains("parent cycle")),
        "{violations:?}"
    );
}

#[test]
fn checker_reports_failure_outside_selection() {
    let fake = Fake {
        failure_outside_selection: true,
        ..Fake::default()
    };
    let violations = check_provider(Arc::new(fake));
    assert!(
        violations
            .iter()
            .any(|v| v.contains("which the selection does not cover")),
        "{violations:?}"
    );
}

#[test]
fn checker_reports_panics_instead_of_unwinding() {
    let fake = Fake {
        panic_on_describe: true,
        ..Fake::default()
    };
    let violations = check_provider(Arc::new(fake));
    assert!(
        violations
            .iter()
            .any(|v| v.contains("describe") && v.contains("panicked")),
        "{violations:?}"
    );
}

struct WireExtension;

impl Extension for WireExtension {
    fn new() -> Self {
        WireExtension
    }

    fn provider(&self, id: &str) -> Option<Box<dyn Provider>> {
        (id == "fake").then(|| Box::new(Fake::default()) as Box<dyn Provider>)
    }
}

struct NoHost;

impl Host for NoHost {
    fn log(&self, _level: LogLevel, _message: &str) {}
    fn data_dir(&self) -> PathBuf {
        PathBuf::new()
    }
    fn setting(&self, _key: &str) -> Option<String> {
        None
    }
    fn secret(&self, _key: &str) -> Option<String> {
        None
    }
    fn set_secret(&self, _key: &str, _value: Option<&str>) -> Result<(), ProviderError> {
        Ok(())
    }
    fn open_url(&self, _url: &str) -> Result<(), ProviderError> {
        Ok(())
    }
    fn cancelled(&self) -> bool {
        false
    }
}

#[test]
fn extreme_deadline_is_accepted_without_panicking() {
    let request = format!(
        r#"{{"provider":"fake","deadline_ms":{},"args":null}}"#,
        u64::MAX
    );
    let response = dispatch(
        &WireExtension,
        &NoHost,
        Method::Info as u32,
        request.as_bytes(),
    );
    let outcome: Result<ProviderInfo, ProviderError> =
        serde_json::from_slice(&response).expect("response decodes");
    assert!(outcome.is_ok(), "{outcome:?}");
}
