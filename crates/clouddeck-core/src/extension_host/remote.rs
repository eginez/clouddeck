use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use clouddeck_extension_api::wire::{Method, Request};
use clouddeck_provider_api::{
    AuthAnswer, AuthInput, AuthMethod, AuthStatus, AuthStep, Call, CellFailure, Connection,
    ConnectionId, Dimension, ErrorKind, ExtrasSection, FieldDef, FlowId, InspectorSectionDef,
    Instance, KindId, KindInfo, ListResult, Provider, ProviderError, ProviderInfo, ResourceId,
    Selection,
};
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::library::{HostContext, Library, with_cancel_flag};
use super::manifest::Manifest;
use super::state::{CRASH_LIMIT, State, StateStore};

const DEFAULT_BUDGET: Duration = Duration::from_secs(30);
const CONNECTION_LABEL: &str = "Profile";

/// One installed extension: its manifest, its state, and its library once loaded.
pub struct Plugin {
    root: PathBuf,
    dir: PathBuf,
    manifest: Manifest,
    state: Arc<StateStore>,
    host: Arc<HostContext>,
    library: Mutex<Option<Arc<Library>>>,
    loaded: AtomicBool,
}

impl Plugin {
    pub fn new(root: PathBuf, dir: PathBuf, manifest: Manifest, state: Arc<StateStore>) -> Self {
        let host = Arc::new(HostContext::new(
            manifest.id.clone(),
            dir.join("data"),
            manifest.capabilities.clone(),
            Arc::clone(&state),
        ));
        Self {
            root,
            dir,
            manifest,
            state,
            host,
            library: Mutex::new(None),
            loaded: AtomicBool::new(false),
        }
    }

    fn load(&self) -> Result<Arc<Library>, ProviderError> {
        let mut slot = lock(&self.library);
        if let Some(library) = slot.as_ref() {
            return Ok(Arc::clone(library));
        }
        let allowed = self
            .state
            .update(State::begin_launch)
            .map_err(|error| self.failure(format!("cannot record launch: {error}")))?;
        if !allowed {
            return Err(self.failure(format!(
                "disabled after crashing on its last {CRASH_LIMIT} launches"
            )));
        }
        let opened = self
            .manifest
            .library_path(&self.dir)
            .and_then(|path| Library::open(&path, &self.root, Arc::clone(&self.host)));
        match opened {
            Ok(library) => {
                let library = Arc::new(library);
                *slot = Some(Arc::clone(&library));
                self.loaded.store(true, Ordering::Release);
                Ok(library)
            }
            Err(message) => {
                self.settle(State::launch_ended);
                Err(self.failure(message))
            }
        }
    }

    fn failure(&self, message: String) -> ProviderError {
        ProviderError::new(ErrorKind::Other, format!("{}: {message}", self.manifest.id))
    }

    fn settle(&self, change: fn(&mut State)) {
        if let Err(error) = self.state.update(change) {
            eprintln!(
                "[extension {}] cannot save launch state: {error}",
                self.manifest.id
            );
        }
    }

    fn execute(
        &self,
        method: u32,
        request: &[u8],
        cancel: Arc<AtomicBool>,
    ) -> Result<Vec<u8>, ProviderError> {
        let library = self.load()?;
        with_cancel_flag(cancel, || library.call(method, request))
    }

    /// Runs one call on a worker thread. On deadline the worker is told to cancel and the caller
    /// gets a timeout; the library cannot be interrupted, so the worker finishes on its own.
    fn run(
        self: &Arc<Self>,
        method: u32,
        request: Vec<u8>,
        budget: Duration,
    ) -> Result<Vec<u8>, ProviderError> {
        if budget.is_zero() {
            return Err(ProviderError::of(ErrorKind::Timeout));
        }
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker = Arc::clone(self);
        let worker_cancel = Arc::clone(&cancel);
        thread::Builder::new()
            .name(format!("extension-{}", self.manifest.id))
            .spawn(move || {
                let outcome = worker.execute(method, &request, worker_cancel);
                let _ = sender.send(outcome);
            })
            .map_err(|error| self.failure(format!("cannot start a call thread: {error}")))?;
        match receiver.recv_timeout(budget) {
            Ok(outcome) => outcome,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                cancel.store(true, Ordering::Relaxed);
                Err(ProviderError::of(ErrorKind::Timeout))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(self.failure("the call ended without a response".into()))
            }
        }
    }

    /// Records a clean shutdown if the library was loaded. Never takes the library lock, which a
    /// hung load may hold.
    pub fn shutdown(&self) {
        if self.loaded.load(Ordering::Acquire) {
            self.settle(State::clean_shutdown);
        }
    }

    pub fn version(&self) -> &str {
        &self.manifest.version
    }

    pub fn id(&self) -> &str {
        &self.manifest.id
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A provider served by an extension. Loading the library waits for the first call.
pub struct RemoteProvider {
    plugin: Arc<Plugin>,
    provider: String,
    name: String,
    info: OnceLock<ProviderInfo>,
}

impl RemoteProvider {
    pub fn new(plugin: Arc<Plugin>, provider: String, name: String) -> Self {
        Self {
            plugin,
            provider,
            name,
            info: OnceLock::new(),
        }
    }

    fn invoke<A: Serialize, R: DeserializeOwned>(
        &self,
        deadline: Option<Instant>,
        method: Method,
        args: A,
    ) -> Result<R, ProviderError> {
        let budget = deadline.map_or(DEFAULT_BUDGET, |at| {
            at.saturating_duration_since(Instant::now())
        });
        let request = Request {
            provider: self.provider.clone(),
            deadline_ms: deadline.map(|_| budget.as_millis() as u64),
            args,
        };
        let body = serde_json::to_vec(&request).map_err(|error| {
            ProviderError::new(
                ErrorKind::Other,
                format!("request could not be encoded: {error}"),
            )
        })?;
        let response = self.plugin.run(method as u32, body, budget)?;
        let outcome: Result<R, ProviderError> =
            serde_json::from_slice(&response).map_err(|error| {
                ProviderError::new(
                    ErrorKind::Other,
                    format!("{} sent a malformed response: {error}", self.plugin.id()),
                )
            })?;
        outcome
    }

    fn fallible<T: Default>(&self, result: Result<T, ProviderError>) -> T {
        result.unwrap_or_else(|error| {
            eprintln!("[{}] {error}", self.provider);
            T::default()
        })
    }
}

impl Provider for RemoteProvider {
    fn info(&self) -> ProviderInfo {
        if let Some(info) = self.info.get() {
            return info.clone();
        }
        match self.invoke::<(), ProviderInfo>(None, Method::Info, ()) {
            Ok(info) => {
                let _ = self.info.set(info.clone());
                info
            }
            Err(error) => {
                eprintln!("[{}] {error}", self.provider);
                ProviderInfo {
                    id: self.provider.clone(),
                    name: self.name.clone(),
                    version: self.plugin.version().to_string(),
                    connection_label: CONNECTION_LABEL.into(),
                }
            }
        }
    }

    fn connections(&self, cx: &Call<'_>) -> Result<Vec<Connection>, ProviderError> {
        self.invoke(cx.deadline, Method::Connections, ())
    }

    fn dimensions(
        &self,
        cx: &Call<'_>,
        connection: &ConnectionId,
    ) -> Result<Vec<Dimension>, ProviderError> {
        self.invoke(cx.deadline, Method::Dimensions, connection)
    }

    fn kinds(&self) -> Vec<KindInfo> {
        self.fallible(self.invoke(None, Method::Kinds, ()))
    }

    fn list(
        &self,
        cx: &Call<'_>,
        connection: &ConnectionId,
        kind: &KindId,
        selection: &Selection,
    ) -> ListResult {
        self.invoke(cx.deadline, Method::List, (connection, kind, selection))
            .unwrap_or_else(|error| ListResult {
                resources: Vec::new(),
                failures: vec![CellFailure {
                    cell: selection.clone(),
                    error,
                }],
            })
    }

    fn describe(
        &self,
        cx: &Call<'_>,
        connection: &ConnectionId,
        kind: &KindId,
        id: &ResourceId,
    ) -> Result<Vec<ExtrasSection>, ProviderError> {
        self.invoke(cx.deadline, Method::Describe, (connection, kind, id))
    }

    fn auth_methods(&self) -> Vec<AuthMethod> {
        self.fallible(self.invoke(None, Method::AuthMethods, ()))
    }

    fn begin_auth(
        &self,
        cx: &Call<'_>,
        method: &AuthMethod,
        input: AuthInput,
    ) -> Result<AuthStep, ProviderError> {
        self.invoke(cx.deadline, Method::BeginAuth, (method, input))
    }

    fn continue_auth(
        &self,
        cx: &Call<'_>,
        flow: &FlowId,
        answer: AuthAnswer,
    ) -> Result<AuthStep, ProviderError> {
        self.invoke(cx.deadline, Method::ContinueAuth, (flow, answer))
    }

    fn auth_status(&self, cx: &Call<'_>, connection: Option<&ConnectionId>) -> AuthStatus {
        self.invoke(cx.deadline, Method::AuthStatus, connection)
            .unwrap_or(AuthStatus::SignedOut)
    }

    fn sign_out(
        &self,
        cx: &Call<'_>,
        connection: Option<&ConnectionId>,
    ) -> Result<(), ProviderError> {
        self.invoke(cx.deadline, Method::SignOut, connection)
    }

    fn console_url(
        &self,
        connection: &ConnectionId,
        kind: &KindId,
        resource: &Instance,
    ) -> Option<String> {
        self.invoke::<_, Option<String>>(None, Method::ConsoleUrl, (connection, kind, resource))
            .ok()
            .flatten()
    }

    fn fields(&self, kind: &KindId) -> Vec<FieldDef> {
        self.fallible(self.invoke(None, Method::Fields, kind))
    }

    fn inspector(&self, kind: &KindId) -> Vec<InspectorSectionDef> {
        self.fallible(self.invoke(None, Method::Inspector, kind))
    }
}
