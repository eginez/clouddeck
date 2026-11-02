//! Checks a provider against the contract v1 obligations, for use from an extension's own tests.

use std::collections::{BTreeMap, BTreeSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use clouddeck_provider_api::{
    AuthAnswer, AuthInput, AuthMethod, AuthStep, Call, ConnectionId, DimensionId, ErrorKind,
    FlowId, Host, Instance, KindId, LogLevel, Provider, ProviderError, ResourceId, Selection,
    ValueId,
};

const MISSING: &str = "conformance-no-such-value";
const CANCELLED_LIST_LIMIT: Duration = Duration::from_secs(5);

struct KitHost {
    cancelled: bool,
}

impl Host for KitHost {
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
        self.cancelled
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

/// Runs every contract v1 obligation against `p` and returns the violations; empty means conformant.
pub fn check_provider(shared: Arc<dyn Provider>) -> Vec<String> {
    let p: &dyn Provider = &*shared;
    let host = KitHost { cancelled: false };
    let cx = Call {
        deadline: None,
        host: &host,
    };
    let mut r = Report(Vec::new());
    r.guard("info()", || p.info());

    let kinds = check_kinds(p, &mut r);
    let connections = check_connections(p, &cx, &mut r);
    let mut listing = Vec::new();
    for (index, conn) in connections.iter().enumerate() {
        let found = check_connection(p, &cx, &mut r, conn, &kinds);
        if index == 0 {
            listing = found;
        }
    }
    if let Some(conn) = connections.first() {
        check_resource_views(p, &cx, &mut r, conn, &kinds, &listing);
        if let Some(kind) = kinds.first() {
            check_deadline(Arc::clone(&shared), &mut r, conn, kind);
        }
    }
    check_auth(p, &cx, &mut r);
    for kind in &kinds {
        let defs = r.guard("fields()", || p.fields(kind)).unwrap_or_default();
        r.unique(&format!("fields({kind})"), defs.into_iter().map(|d| d.name));
    }
    r.0
}

fn check_kinds(p: &dyn Provider, r: &mut Report) -> Vec<KindId> {
    let infos = r.guard("kinds()", || p.kinds()).unwrap_or_default();
    if infos.iter().any(|k| k.id.as_str().is_empty()) {
        r.fail("kinds() has an empty id");
    }
    r.unique("kinds()", infos.iter().map(|k| k.id.to_string()));
    infos.into_iter().map(|k| k.id).collect()
}

fn check_connections(p: &dyn Provider, cx: &Call<'_>, r: &mut Report) -> Vec<ConnectionId> {
    match r.guard("connections()", || p.connections(cx)) {
        Some(Ok(list)) => {
            r.unique("connections()", list.iter().map(|c| c.id.to_string()));
            list.into_iter().map(|c| c.id).collect()
        }
        Some(Err(error)) if error.kind != ErrorKind::Auth => {
            r.fail(format!("connections() failed: {error}"));
            Vec::new()
        }
        _ => Vec::new(),
    }
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

fn check_deadline(p: Arc<dyn Provider>, r: &mut Report, conn: &ConnectionId, kind: &KindId) {
    let (conn, kind) = (conn.clone(), kind.clone());
    let (tx, rx) = mpsc::channel();
    // A provider that never returns leaves this thread running; the check still reports it and finishes.
    thread::spawn(move || {
        let host = KitHost { cancelled: true };
        let cx = Call {
            deadline: Some(Instant::now()),
            host: &host,
        };
        let finished = catch_unwind(AssertUnwindSafe(|| {
            p.list(&cx, &conn, &kind, &Selection::new())
        }));
        let _ = tx.send(finished.is_ok());
    });
    match rx.recv_timeout(CANCELLED_LIST_LIMIT) {
        Ok(true) => {}
        Ok(false) | Err(RecvTimeoutError::Disconnected) => {
            r.fail("list() after cancellation panicked");
        }
        Err(RecvTimeoutError::Timeout) => r.fail(format!(
            "list() did not return within {CANCELLED_LIST_LIMIT:?} after the host cancelled; it must check cancelled() and return promptly"
        )),
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
        if flow.as_str().is_empty() {
            r.fail(format!("auth method {} issued an empty FlowId", method.id));
            return;
        }
        let Some(next) = r.ok("continue_auth()", || p.continue_auth(cx, &flow, answer)) else {
            return;
        };
        step = next;
    }
}
