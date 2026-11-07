//! Checks a provider against the contract v1 obligations, for use from an extension's own tests.

use std::collections::{BTreeMap, BTreeSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use clouddeck_provider_api::{
    AuthAnswer, AuthInput, AuthMethod, AuthStatus, AuthStep, Call, ConnectionId, Dimension,
    DimensionId, ErrorKind, FlowId, Host, Instance, KindId, ListResult, LogLevel, Provider,
    ProviderError, ResourceId, Selection, ValueId,
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
        if has_cycle(dim) {
            r.fail(format!("dimension {} has a parent cycle", dim.id));
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
    for kind in kinds {
        for dim in &dims {
            check_selection_tree(p, cx, r, conn, kind, dim);
        }
    }
    listing
}

fn is_parent(dim: &Dimension, value: &ValueId) -> bool {
    dim.values.iter().any(|v| v.parent.as_ref() == Some(value))
}

fn children(dim: &Dimension, value: &ValueId) -> Vec<ValueId> {
    dim.values
        .iter()
        .filter(|v| v.parent.as_ref() == Some(value))
        .map(|v| v.id.clone())
        .collect()
}

fn has_cycle(dim: &Dimension) -> bool {
    let parents: BTreeMap<&ValueId, Option<&ValueId>> = dim
        .values
        .iter()
        .map(|v| (&v.id, v.parent.as_ref()))
        .collect();
    dim.values.iter().any(|start| {
        let mut current = parents.get(&start.id).copied().flatten();
        for _ in 0..dim.values.len() {
            match current {
                None => return false,
                Some(id) if id == &start.id => return true,
                Some(id) => current = parents.get(id).copied().flatten(),
            }
        }
        true
    })
}

fn covered(dim: &Dimension, selected: &[ValueId]) -> BTreeSet<ValueId> {
    let mut out: BTreeSet<ValueId> = selected.iter().cloned().collect();
    let mut frontier = selected.to_vec();
    while let Some(value) = frontier.pop() {
        for child in children(dim, &value) {
            if out.insert(child.clone()) {
                frontier.push(child);
            }
        }
    }
    out
}

fn check_failure_cells(r: &mut Report, dim: &Dimension, selection: &Selection, list: &ListResult) {
    let allowed = covered(dim, selection.values(dim.id.as_str()));
    for failure in &list.failures {
        for value in failure.cell.values(dim.id.as_str()) {
            if !allowed.contains(value) {
                r.fail(format!(
                    "dimension {}: CellFailure names {value}, which the selection does not cover",
                    dim.id
                ));
            }
            if is_parent(dim, value) {
                r.fail(format!(
                    "dimension {}: CellFailure names {value}, which is not a finest-level value",
                    dim.id
                ));
            }
        }
    }
}

fn check_selection_tree(
    p: &dyn Provider,
    cx: &Call<'_>,
    r: &mut Report,
    conn: &ConnectionId,
    kind: &KindId,
    dim: &Dimension,
) {
    if dim.values.iter().all(|v| v.parent.is_none()) || has_cycle(dim) {
        return;
    }
    let mut members: BTreeMap<ValueId, BTreeSet<String>> = BTreeMap::new();
    for value in &dim.values {
        let selection = Selection::new().with(dim.id.clone(), vec![value.id.clone()]);
        let Some(list) = r.guard("list() for a dimension value", || {
            p.list(cx, conn, kind, &selection)
        }) else {
            return;
        };
        check_failure_cells(r, dim, &selection, &list);
        let ids = list.resources.iter().map(|i| i.id.clone()).collect();
        members.insert(value.id.clone(), ids);
    }

    let leaves: Vec<&ValueId> = dim
        .values
        .iter()
        .map(|v| &v.id)
        .filter(|v| !is_parent(dim, v))
        .collect();
    for (i, a) in leaves.iter().enumerate() {
        for b in &leaves[i + 1..] {
            if !members[*a].is_disjoint(&members[*b]) {
                r.fail(format!(
                    "dimension {}: {a} and {b} both list the same resources",
                    dim.id
                ));
            }
        }
    }

    for parent in dim
        .values
        .iter()
        .map(|v| &v.id)
        .filter(|v| is_parent(dim, v))
    {
        let kids = children(dim, parent);
        let under_children: BTreeSet<String> = kids
            .iter()
            .flat_map(|c| members[c].iter().cloned())
            .collect();
        if members[parent] != under_children {
            r.fail(format!(
                "dimension {}: selecting {parent} does not list exactly its children's resources",
                dim.id
            ));
        }
        let mut both = kids;
        both.insert(0, parent.clone());
        let selection = Selection::new().with(dim.id.clone(), both);
        if let Some(list) = r.guard("list() for a parent with its children", || {
            p.list(cx, conn, kind, &selection)
        }) {
            check_failure_cells(r, dim, &selection, &list);
            let ids: BTreeSet<String> = list.resources.iter().map(|i| i.id.clone()).collect();
            if ids != members[parent] {
                r.fail(format!(
                    "dimension {}: selecting {parent} with its children lists different resources than {parent} alone",
                    dim.id
                ));
            }
        }
    }
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
        check_auth_flow(p, cx, r, None, method);
        check_auth_flow(p, cx, r, Some(&conn), method);
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

fn is_signed_out(p: &dyn Provider, cx: &Call<'_>, conn: Option<&ConnectionId>) -> bool {
    matches!(
        catch_unwind(AssertUnwindSafe(|| p.auth_status(cx, conn))),
        Ok(AuthStatus::SignedOut)
    )
}

fn check_auth_flow(
    p: &dyn Provider,
    cx: &Call<'_>,
    r: &mut Report,
    conn: Option<&ConnectionId>,
    method: &AuthMethod,
) {
    let input = AuthInput {
        connection: conn.cloned(),
        values: BTreeMap::new(),
    };
    let what = if conn.is_some() {
        "begin_auth()"
    } else {
        "begin_auth(None)"
    };
    let begun = match r.guard(what, || p.begin_auth(cx, method, input)) {
        Some(Ok(step)) => Some(step),
        Some(Err(error)) if error.kind == ErrorKind::Auth && is_signed_out(p, cx, conn) => None,
        Some(Err(error)) if conn.is_none() && error.kind == ErrorKind::Unsupported => None,
        Some(Err(error)) => {
            r.fail(format!("{what} failed: {error}"));
            None
        }
        None => None,
    };
    let Some(mut step) = begun else {
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

#[cfg(test)]
mod tests {
    use super::*;
    use clouddeck_provider_api::{CellFailure, DimValue};

    #[test]
    fn whole_call_failure_with_empty_cell_is_accepted() {
        let dim = Dimension {
            id: "region".into(),
            label: "Region".into(),
            plural_label: None,
            multi: true,
            optional: true,
            values: vec![DimValue {
                id: "us".into(),
                label: "us".into(),
                parent: None,
                default: false,
            }],
        };
        let selection = Selection::new().with("region", vec!["us".into()]);
        let list = ListResult {
            resources: vec![],
            failures: vec![CellFailure {
                cell: Selection::new(),
                error: ProviderError::of(ErrorKind::Other),
            }],
        };
        let mut r = Report(Vec::new());
        check_failure_cells(&mut r, &dim, &selection, &list);
        assert!(r.0.is_empty(), "{:?}", r.0);
    }
}
