use std::collections::{BTreeMap, BTreeSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

use clouddeck_provider_api::{
    AuthAnswer, AuthInput, AuthMethod, AuthStep, Call, CellFailure, Connection, ConnectionId,
    CoreField, DimValue, Dimension, DimensionId, ErrorKind, ExtrasSection, FieldDef, FieldKind,
    FieldSource, FlowId, Host, INSTANCES, Instance, KindId, KindInfo, ListResult, LogLevel,
    MatchMode, PromptField, Provider, ProviderError, ProviderInfo, ResourceId, Selection, ValueId,
};

const REGIONS: [&str; 3] = ["us", "us-east", "eu"];
const MISSING: &str = "conformance-no-such-value";

struct NullHost;

impl Host for NullHost {
    fn log(&self, _: LogLevel, _: &str) {}
    fn data_dir(&self) -> PathBuf {
        PathBuf::new()
    }
    fn setting(&self, _: &str) -> Option<String> {
        None
    }
    fn secret(&self, _: &str) -> Option<String> {
        None
    }
    fn set_secret(&self, _: &str, _: Option<&str>) -> Result<(), ProviderError> {
        Ok(())
    }
    fn open_url(&self, _: &str) -> Result<(), ProviderError> {
        Ok(())
    }
    fn cancelled(&self) -> bool {
        false
    }
}

#[derive(Default)]
struct Fake {
    duplicate_ids: bool,
    panic_on_describe: bool,
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

    fn connections(&self, _: &Call<'_>) -> Result<Vec<Connection>, ProviderError> {
        Ok(vec![Connection {
            id: "default".into(),
            display_name: "Default".into(),
        }])
    }

    fn dimensions(&self, _: &Call<'_>, _: &ConnectionId) -> Result<Vec<Dimension>, ProviderError> {
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
                        section: "Details",
                        key: "AMI",
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

struct Report(Vec<String>);

impl Report {
    fn fail(&mut self, message: impl Into<String>) {
        self.0.push(message.into());
    }

    fn guard<T>(&mut self, what: &str, call: impl FnOnce() -> T) -> Option<T> {
        let result = catch_unwind(AssertUnwindSafe(call));
        if result.is_err() {
            self.fail(format!("{what} panicked"));
        }
        result.ok()
    }

    fn ok<T>(&mut self, what: &str, call: impl FnOnce() -> Result<T, ProviderError>) -> Option<T> {
        match self.guard(what, call)? {
            Ok(value) => Some(value),
            Err(error) => {
                self.fail(format!("{what} failed: {error}"));
                None
            }
        }
    }

    fn unique(&mut self, what: &str, ids: impl IntoIterator<Item = String>) {
        let mut seen = BTreeSet::new();
        for id in ids {
            if !seen.insert(id.clone()) {
                self.fail(format!("{what}: duplicate id {id:?}"));
            }
        }
    }
}

/// Runs every contract v1 obligation against `p` and returns the violations found.
pub fn check_provider(p: &dyn Provider) -> Vec<String> {
    let host = NullHost;
    let cx = Call {
        deadline: None,
        host: &host,
    };
    let mut r = Report(Vec::new());
    r.guard("info()", || p.info());

    let kind_infos = r.guard("kinds()", || p.kinds()).unwrap_or_default();
    if kind_infos.is_empty() || kind_infos.iter().any(|k| k.id.as_str().is_empty()) {
        r.fail("kinds() is empty or has an empty id");
    }
    r.unique("kinds()", kind_infos.iter().map(|k| k.id.to_string()));
    let kinds: Vec<KindId> = kind_infos.into_iter().map(|k| k.id).collect();

    let connections = match r.guard("connections()", || p.connections(&cx)) {
        Some(Ok(list)) => {
            r.unique("connections()", list.iter().map(|c| c.id.to_string()));
            list.into_iter().map(|c| c.id).collect()
        }
        Some(Err(error)) if error.kind != ErrorKind::Auth => {
            r.fail(format!("connections() failed: {error}"));
            Vec::new()
        }
        _ => Vec::new(),
    };

    let mut listing = Vec::new();
    for (index, conn) in connections.iter().enumerate() {
        let found = check_connection(p, &cx, &mut r, conn, &kinds);
        if index == 0 {
            listing = found;
        }
    }
    if let Some(conn) = connections.first() {
        check_resource_views(p, &cx, &mut r, conn, &kinds, &listing);
    }
    check_auth(p, &cx, &mut r);
    for kind in &kinds {
        let defs = r.guard("fields()", || p.fields(kind)).unwrap_or_default();
        r.unique(&format!("fields({kind})"), defs.into_iter().map(|d| d.name));
    }
    r.0
}

fn check_connection(
    p: &dyn Provider,
    cx: &Call<'_>,
    r: &mut Report,
    conn: &ConnectionId,
    kinds: &[KindId],
) -> Vec<(KindId, Instance)> {
    let dims = r
        .ok("dimensions()", || p.dimensions(cx, conn))
        .unwrap_or_default();
    r.unique("dimensions()", dims.iter().map(|d| d.id.to_string()));
    for dim in &dims {
        r.unique(
            &format!("dimension {}", dim.id),
            dim.values.iter().map(|v| v.id.to_string()),
        );
        let ids: BTreeSet<&ValueId> = dim.values.iter().map(|v| &v.id).collect();
        if dim.values.iter().any(|v| {
            v.parent
                .as_ref()
                .is_some_and(|parent| !ids.contains(parent))
        }) {
            r.fail(format!(
                "dimension {} has a value with an unknown parent",
                dim.id
            ));
        }
    }

    let mut listing = Vec::new();
    for kind in kinds {
        let Some(list) = r.guard("list()", || p.list(cx, conn, kind, &Selection::new())) else {
            continue;
        };
        r.unique(
            "list() resource ids",
            list.resources.iter().map(|i| i.id.clone()),
        );
        listing.extend(list.resources.into_iter().map(|i| (kind.clone(), i)));
        if let Some(dim) = dims.first() {
            check_bad_selection(p, cx, r, conn, kind, &dim.id);
        }
    }
    listing
}

fn check_bad_selection(
    p: &dyn Provider,
    cx: &Call<'_>,
    r: &mut Report,
    conn: &ConnectionId,
    kind: &KindId,
    dim: &DimensionId,
) {
    let bad = ValueId::from(MISSING);
    let selection = Selection::new().with(dim.clone(), vec![bad.clone()]);
    let Some(list) = r.guard("list() with a bad selection", || {
        p.list(cx, conn, kind, &selection)
    }) else {
        return;
    };
    if list.failures.is_empty() {
        r.fail(format!(
            "list() with unknown {dim} value returned no CellFailure"
        ));
    } else if list
        .failures
        .iter()
        .any(|f| !f.cell.values(dim.as_str()).contains(&bad))
    {
        r.fail("CellFailure does not name the failing dimension value");
    }
}

fn check_resource_views(
    p: &dyn Provider,
    cx: &Call<'_>,
    r: &mut Report,
    conn: &ConnectionId,
    kinds: &[KindId],
    listing: &[(KindId, Instance)],
) {
    if let Some((kind, first)) = listing.first() {
        let id = ResourceId::from(first.id.as_str());
        if let Some(Err(error)) = r.guard("describe() on a known id", || {
            p.describe(cx, conn, kind, &id)
        }) {
            r.fail(format!("describe() on a known id failed: {error}"));
        }
    }
    if let Some(kind) = kinds.first() {
        let id = ResourceId::from(MISSING);
        match r.guard("describe() on an unknown id", || {
            p.describe(cx, conn, kind, &id)
        }) {
            Some(Ok(_)) => r.fail("describe() accepted an unknown id"),
            Some(Err(error))
                if !matches!(error.kind, ErrorKind::NotFound | ErrorKind::Unsupported) =>
            {
                r.fail(format!(
                    "describe() on an unknown id returned {:?}",
                    error.kind
                ));
            }
            _ => {}
        }
    }
    let samples: Vec<(KindId, Instance)> = match listing.is_empty() {
        true => kinds
            .iter()
            .map(|k| (k.clone(), Instance::default()))
            .collect(),
        false => listing.to_vec(),
    };
    for (kind, instance) in &samples {
        r.guard("console_url()", || p.console_url(conn, kind, instance));
    }
}

fn check_auth(p: &dyn Provider, cx: &Call<'_>, r: &mut Report) {
    let conn = ConnectionId::from("conformance");
    let methods = r
        .guard("auth_methods()", || p.auth_methods())
        .unwrap_or_default();
    r.unique("auth_methods()", methods.iter().map(|m| m.id.clone()));
    for method in &methods {
        check_auth_flow(p, cx, r, &conn, method);
    }
    let unknown = FlowId::from("conformance-no-such-flow");
    if let Some(Ok(_)) = r.guard("continue_auth() unknown FlowId", || {
        p.continue_auth(cx, &unknown, AuthAnswer::Poll)
    }) {
        r.fail("continue_auth() accepted an unknown FlowId");
    }
    r.guard("auth_status()", || p.auth_status(cx, Some(&conn)));
    r.guard("sign_out()", || p.sign_out(cx, Some(&conn)));
}

fn check_auth_flow(
    p: &dyn Provider,
    cx: &Call<'_>,
    r: &mut Report,
    conn: &ConnectionId,
    method: &AuthMethod,
) {
    let input = AuthInput {
        connection: conn.clone(),
        values: BTreeMap::new(),
    };
    let Some(mut step) = r.ok("begin_auth()", || p.begin_auth(cx, method, input)) else {
        return;
    };
    for _ in 0..4 {
        let (flow, answer) = match &step {
            AuthStep::Done => return,
            AuthStep::Prompt { flow, fields } => (
                flow.clone(),
                AuthAnswer::Values(fields.iter().map(|f| (f.id.clone(), "x".into())).collect()),
            ),
            AuthStep::OpenUrl { flow, .. } | AuthStep::DeviceCode { flow, .. } => {
                (flow.clone(), AuthAnswer::Poll)
            }
        };
        let Some(next) = r.ok("continue_auth()", || p.continue_auth(cx, &flow, answer)) else {
            return;
        };
        step = next;
    }
    if !matches!(step, AuthStep::Done) {
        r.fail(format!("auth method {} did not reach Done", method.id));
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
