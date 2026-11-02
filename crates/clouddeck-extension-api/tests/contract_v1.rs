use clouddeck_extension_api::conformance::check_provider;
use clouddeck_extension_api::{
    AuthAnswer, AuthInput, AuthMethod, AuthStatus, AuthStep, Call, CellFailure, Connection,
    ConnectionId, CoreField, DimValue, Dimension, ErrorKind, ExtrasSection, FieldDef, FieldKind,
    FieldSource, FlowId, INSTANCES, Instance, KindId, KindInfo, ListResult, MatchMode,
    PromptField, Provider, ProviderError, ProviderInfo, ResourceId, Selection, ValueId,
};

const REGIONS: [&str; 3] = ["us", "us-east", "eu"];

#[derive(Default)]
struct Fake {
    duplicate_ids: bool,
    panic_on_describe: bool,
    signed_out: bool,
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

    fn dimensions(
        &self,
        _: &Call<'_>,
        _: &ConnectionId,
    ) -> Result<Vec<Dimension>, ProviderError> {
        Ok(vec![Dimension {
            id: "region".into(),
            label: "Region".into(),
            multi: true,
            optional: true,
            values: vec![
                value("us", None),
                value("us-east", Some("us")),
                value("eu", None),
            ],
        }])
    }

    fn kinds(&self) -> Vec<KindInfo> {
        vec![KindInfo {
            id: KindId::instances(),
            label: "Instances".into(),
        }]
    }

    fn list(&self, _: &Call<'_>, _: &ConnectionId, kind: &KindId, sel: &Selection) -> ListResult {
        if kind.as_str() != INSTANCES {
            return ListResult::default();
        }
        let regions = sel.values("region");
        let unknown: Vec<ValueId> = regions
            .iter()
            .filter(|r| !REGIONS.contains(&r.as_str()))
            .cloned()
            .collect();
        let failures = match unknown.is_empty() {
            true => vec![],
            false => vec![CellFailure {
                cell: Selection::new().with("region", unknown),
                error: ProviderError::of(ErrorKind::NotFound),
            }],
        };
        let mut resources: Vec<Instance> = instances()
            .into_iter()
            .filter(|i| {
                regions.is_empty()
                    || regions
                        .iter()
                        .any(|r| Some(r.as_str()) == i.region.as_deref())
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
    let violations = check_provider(&Fake::default());
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
    let violations = check_provider(&fake);
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn checker_reports_duplicate_resource_ids() {
    let fake = Fake {
        duplicate_ids: true,
        ..Fake::default()
    };
    let violations = check_provider(&fake);
    assert!(
        violations.iter().any(|v| v.contains("duplicate")),
        "{violations:?}"
    );
}

#[test]
fn checker_reports_panics_instead_of_unwinding() {
    let fake = Fake {
        panic_on_describe: true,
        ..Fake::default()
    };
    let violations = check_provider(&fake);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("describe") && v.contains("panicked")),
        "{violations:?}"
    );
}
