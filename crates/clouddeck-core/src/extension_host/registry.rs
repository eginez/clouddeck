use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clouddeck_extension_api::API_VERSION;
use clouddeck_provider_api::Provider;

use crate::builtin_providers;
use crate::context::config_home;
use crate::extension_manager::install::EXTERNAL_EXTENSIONS_ALLOWED;

use super::manifest::{MANIFEST_FILE, Manifest};
use super::remote::{NOT_RESPONDING, Plugin, RemoteProvider};
use super::state::{CRASH_LIMIT, State, StateStore, state_path};

const CLOUDDECK_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub extension: String,
    pub message: String,
}

#[derive(Default)]
pub struct ProviderRegistry {
    providers: Vec<Arc<dyn Provider>>,
    plugins: Vec<Arc<Plugin>>,
    provider_ids: BTreeSet<String>,
    refusals: Vec<Refusal>,
    allow_external: bool,
}

impl ProviderRegistry {
    pub fn with_builtins(demo: bool) -> Self {
        let mut registry = Self::default().with_external(EXTERNAL_EXTENSIONS_ALLOWED);
        for provider in builtin_providers(demo) {
            registry
                .register_builtin(provider)
                .expect("builtin provider ids are unique");
        }
        registry
    }

    pub fn register_builtin(&mut self, provider: Arc<dyn Provider>) -> Result<(), String> {
        let id = provider.info().id;
        if self.provider_ids.contains(&id) {
            return Err(format!("provider {id} is already registered"));
        }
        self.provider_ids.insert(id);
        self.providers.push(provider);
        Ok(())
    }

    pub fn with_external(mut self, allowed: bool) -> Self {
        self.allow_external = allowed;
        self
    }

    pub fn providers(&self) -> &[Arc<dyn Provider>] {
        &self.providers
    }

    /// Records a clean shutdown for every extension whose library was loaded this session.
    /// The app quit path must call this, or each loaded extension counts a crash next launch.
    pub fn shutdown(&self) {
        for plugin in &self.plugins {
            plugin.shutdown();
        }
    }

    /// Extensions refused at discovery, plus any whose hung call has not yet returned.
    pub fn refusals(&self) -> Vec<Refusal> {
        let mut refusals = self.refusals.clone();
        refusals.extend(
            self.plugins
                .iter()
                .filter(|plugin| plugin.unhealthy())
                .map(|plugin| Refusal {
                    extension: plugin.id().to_string(),
                    message: NOT_RESPONDING.to_string(),
                }),
        );
        refusals
    }

    /// Reads each installed extension's manifest and host-owned state. No library is loaded here.
    pub fn discover(&mut self, root: &Path, state_root: &Path) -> io::Result<()> {
        if !root.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the extensions root must be an absolute path",
            ));
        }
        let entries = match fs::read_dir(root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        let mut dirs: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_dir() && !is_hidden(path))
            .collect();
        dirs.sort();
        for dir in dirs {
            if let Err(message) = self.add_extension(root, &dir, state_root) {
                let extension = dir
                    .file_name()
                    .unwrap_or(OsStr::new("?"))
                    .to_string_lossy()
                    .into_owned();
                self.refusals.push(Refusal { extension, message });
            }
        }
        Ok(())
    }

    fn add_extension(&mut self, root: &Path, dir: &Path, state_root: &Path) -> Result<(), String> {
        let text = fs::read_to_string(dir.join(MANIFEST_FILE))
            .map_err(|error| format!("cannot read {MANIFEST_FILE}: {error}"))?;
        let manifest = Manifest::parse(&text)?;
        manifest.check_api(API_VERSION, CLOUDDECK_VERSION)?;
        if dir.file_name() != Some(OsStr::new(&manifest.id)) {
            return Err(format!(
                "{} is installed under a different folder name",
                manifest.id
            ));
        }
        let state = Arc::new(
            StateStore::open(&state_path(state_root, &manifest.id))
                .map_err(|error| format!("cannot read state: {error}"))?,
        );
        if !state.read(|s| s.enabled) {
            return Ok(());
        }
        if !self.allow_external {
            return Err("external extensions are turned off in this build".to_string());
        }
        if state.read(State::crash_disabled) {
            return Err(format!(
                "disabled after crashing on its last {CRASH_LIMIT} launches"
            ));
        }
        self.reserve_provider_ids(&manifest)?;
        self.register_remote(root, dir, manifest, state);
        Ok(())
    }

    fn reserve_provider_ids(&mut self, manifest: &Manifest) -> Result<(), String> {
        if let Some(clash) = manifest
            .providers
            .keys()
            .find(|id| self.provider_ids.contains(*id))
        {
            return Err(format!("provider {clash} is already registered"));
        }
        self.provider_ids.extend(manifest.providers.keys().cloned());
        Ok(())
    }

    fn register_remote(
        &mut self,
        root: &Path,
        dir: &Path,
        manifest: Manifest,
        state: Arc<StateStore>,
    ) {
        let declared: Vec<(String, String)> = manifest
            .providers
            .iter()
            .map(|(id, decl)| (id.clone(), decl.name.clone()))
            .collect();
        let plugin = Arc::new(Plugin::new(
            root.to_path_buf(),
            dir.to_path_buf(),
            manifest,
            state,
        ));
        self.plugins.push(Arc::clone(&plugin));
        for (id, name) in declared {
            let provider = RemoteProvider::new(Arc::clone(&plugin), id, name);
            self.providers.push(Arc::new(provider));
        }
    }
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'))
}

/// Built-ins are registered before discovery so an extension claiming their id is refused.
pub fn app_registry(root: &Path, state_root: &Path) -> ProviderRegistry {
    let mut registry = ProviderRegistry::default().with_external(EXTERNAL_EXTENSIONS_ALLOWED);
    registry
        .register_builtin(crate::aws_provider())
        .expect("aws is the first builtin");
    registry
        .register_builtin(Arc::new(crate::demo_provider()))
        .expect("demo is distinct from aws");
    if let Err(error) = registry.discover(root, state_root) {
        eprintln!("Could not read installed extensions: {error}");
    }
    registry
}

/// Where installed extensions live, next to the rest of CloudDeck's configuration.
pub fn extensions_root() -> PathBuf {
    config_home().join("clouddeck/extensions")
}

/// Host-owned state for every extension, kept out of the extension folders.
pub fn extension_state_root() -> PathBuf {
    config_home().join("clouddeck/extension-state")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_text(id: &str, api_version: u32) -> String {
        format!(
            "id = \"{id}\"\nname = \"Test\"\nversion = \"0.1.0\"\nschema_version = 1\napi_version = {api_version}\n\n[providers.{id}]\nname = \"Test cloud\"\n"
        )
    }

    fn install(root: &Path, dir: &str, manifest: &str) -> PathBuf {
        let path = root.join(dir);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join(MANIFEST_FILE), manifest).unwrap();
        path
    }

    fn enable(state_root: &Path, id: &str, change: impl FnOnce(&mut State)) {
        StateStore::open(&state_path(state_root, id))
            .unwrap()
            .update(change)
            .unwrap();
    }

    #[test]
    fn discovery_refuses_mismatched_api_and_names_the_rebuild() {
        let root = tempfile::tempdir().unwrap();
        let state_root = tempfile::tempdir().unwrap();
        install(root.path(), "old", &manifest_text("old", API_VERSION + 1));
        enable(state_root.path(), "old", |s| s.enabled = true);

        let mut registry = ProviderRegistry::default();
        registry.discover(root.path(), state_root.path()).unwrap();

        assert!(registry.providers().is_empty());
        assert_eq!(registry.refusals().len(), 1);
        assert!(
            registry.refusals()[0]
                .message
                .contains("Rebuild old for CloudDeck")
        );
    }

    #[test]
    fn discovery_skips_disabled_extensions_without_refusing_them() {
        let root = tempfile::tempdir().unwrap();
        let state_root = tempfile::tempdir().unwrap();
        install(root.path(), "quiet", &manifest_text("quiet", API_VERSION));

        let mut registry = ProviderRegistry::default();
        registry.discover(root.path(), state_root.path()).unwrap();

        assert!(registry.providers().is_empty());
        assert!(registry.refusals().is_empty());
    }

    #[test]
    fn enabled_external_extension_is_not_loaded_while_external_is_off() {
        let root = tempfile::tempdir().unwrap();
        let state_root = tempfile::tempdir().unwrap();
        install(root.path(), "toy", &manifest_text("toy", API_VERSION));
        enable(state_root.path(), "toy", |s| s.enabled = true);

        let mut registry = ProviderRegistry::default();
        registry.discover(root.path(), state_root.path()).unwrap();

        assert!(registry.providers().is_empty());
        assert_eq!(registry.refusals().len(), 1);
        assert!(
            registry.refusals()[0]
                .message
                .contains("external extensions are turned off in this build")
        );
    }

    #[test]
    fn enabled_external_extension_loads_when_external_is_allowed() {
        let root = tempfile::tempdir().unwrap();
        let state_root = tempfile::tempdir().unwrap();
        install(root.path(), "toy", &manifest_text("toy", API_VERSION));
        enable(state_root.path(), "toy", |s| s.enabled = true);

        let mut registry = ProviderRegistry::default().with_external(true);
        registry.discover(root.path(), state_root.path()).unwrap();

        assert!(registry.refusals().is_empty());
        assert_eq!(registry.providers().len(), 1);
    }

    #[test]
    fn builtin_providers_are_unaffected_by_external_gate() {
        let registry = ProviderRegistry::with_builtins(true);
        assert!(!registry.providers().is_empty());
        assert!(registry.refusals().is_empty());
    }

    #[test]
    fn discover_skips_dot_folders() {
        let root = tempfile::tempdir().unwrap();
        let state_root = tempfile::tempdir().unwrap();
        let dir = install(
            root.path(),
            ".tmp-toy-1",
            &manifest_text("toy", API_VERSION),
        );
        fs::write(dir.join("libtoy.so"), b"").unwrap();
        enable(state_root.path(), "toy", |s| s.enabled = true);

        let mut registry = ProviderRegistry::default();
        registry.discover(root.path(), state_root.path()).unwrap();

        assert!(registry.providers().is_empty());
        assert!(registry.refusals().is_empty());
    }

    #[test]
    fn crash_disabled_extension_is_refused_at_discovery() {
        let root = tempfile::tempdir().unwrap();
        let state_root = tempfile::tempdir().unwrap();
        install(root.path(), "crashy", &manifest_text("crashy", API_VERSION));
        enable(state_root.path(), "crashy", |s| {
            s.enabled = true;
            s.crashes = CRASH_LIMIT;
        });

        let mut registry = ProviderRegistry::default().with_external(true);
        registry.discover(root.path(), state_root.path()).unwrap();

        assert!(registry.providers().is_empty());
        assert!(
            registry.refusals()[0]
                .message
                .contains("crashing on its last")
        );
    }

    #[test]
    fn folder_must_match_manifest_id() {
        let root = tempfile::tempdir().unwrap();
        let state_root = tempfile::tempdir().unwrap();
        install(root.path(), "other", &manifest_text("toy", API_VERSION));
        enable(state_root.path(), "toy", |s| s.enabled = true);

        let mut registry = ProviderRegistry::default();
        registry.discover(root.path(), state_root.path()).unwrap();

        assert!(
            registry.refusals()[0]
                .message
                .contains("different folder name")
        );
    }

    #[test]
    fn shutdown_leaves_unloaded_extensions_alone() {
        let root = tempfile::tempdir().unwrap();
        let state_root = tempfile::tempdir().unwrap();
        install(root.path(), "idle", &manifest_text("idle", API_VERSION));
        enable(state_root.path(), "idle", |s| {
            s.enabled = true;
            s.launch_pending = true;
            s.crashes = 1;
        });

        let mut registry = ProviderRegistry::default().with_external(true);
        registry.discover(root.path(), state_root.path()).unwrap();
        registry.shutdown();

        let state = StateStore::open(&state_path(state_root.path(), "idle")).unwrap();
        assert!(state.read(|s| s.launch_pending));
        assert_eq!(state.read(|s| s.crashes), 1);
    }

    #[test]
    fn state_json_shipped_in_an_extension_folder_is_ignored() {
        let root = tempfile::tempdir().unwrap();
        let state_root = tempfile::tempdir().unwrap();
        let dir = install(root.path(), "toy", &manifest_text("toy", API_VERSION));
        fs::write(
            dir.join("state.json"),
            r#"{"enabled":true,"granted":[{"kind":"credentials"}],"crashes":0}"#,
        )
        .unwrap();

        let mut registry = ProviderRegistry::default();
        registry.discover(root.path(), state_root.path()).unwrap();

        assert!(registry.providers().is_empty());
        assert!(registry.refusals().is_empty());
        let state = StateStore::open(&state_path(state_root.path(), "toy")).unwrap();
        assert!(!state.read(|s| s.enabled));
        assert!(state.read(|s| s.granted.is_empty()));
    }

    #[test]
    fn host_state_is_not_inside_any_extension_folder() {
        let extensions = extensions_root();
        let state = extension_state_root();
        assert!(!state.starts_with(&extensions));
        assert!(!extensions.starts_with(&state));

        let root = tempfile::tempdir().unwrap();
        let dir = install(root.path(), "toy", &manifest_text("toy", API_VERSION));
        let state_file = state_path(&state, "toy");
        assert!(!state_file.starts_with(&dir));
        assert!(!state_file.starts_with(dir.join("data")));
    }

    #[test]
    fn register_builtin_refuses_a_duplicate_id() {
        let mut registry = ProviderRegistry::default();
        registry.register_builtin(Arc::new(crate::demo_provider())).unwrap();

        let error = registry
            .register_builtin(Arc::new(crate::demo_provider()))
            .unwrap_err();

        assert_eq!(error, "provider demo is already registered");
        assert_eq!(registry.providers().len(), 1);
    }

    #[test]
    fn extension_claiming_a_builtin_id_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let state_root = tempfile::tempdir().unwrap();
        install(root.path(), "demo", &manifest_text("demo", API_VERSION));
        enable(state_root.path(), "demo", |s| s.enabled = true);

        let mut registry = ProviderRegistry::default().with_external(true);
        registry.register_builtin(Arc::new(crate::demo_provider())).unwrap();
        registry.discover(root.path(), state_root.path()).unwrap();

        assert_eq!(registry.providers().len(), 1);
        assert_eq!(registry.refusals().len(), 1);
        assert!(
            registry.refusals()[0]
                .message
                .contains("provider demo is already registered")
        );
    }

    #[test]
    fn app_registry_without_extensions_holds_aws_and_demo() {
        let root = tempfile::tempdir().unwrap();
        let state_root = tempfile::tempdir().unwrap();

        let registry = app_registry(&root.path().join("absent"), state_root.path());

        let ids: Vec<String> = registry
            .providers()
            .iter()
            .map(|provider| provider.info().id)
            .collect();
        assert_eq!(ids, ["aws", "demo"]);
        assert!(registry.refusals().is_empty());
    }

    #[test]
    fn missing_extensions_root_is_empty() {
        let root = tempfile::tempdir().unwrap();
        let state_root = tempfile::tempdir().unwrap();
        let mut registry = ProviderRegistry::default();
        registry
            .discover(&root.path().join("absent"), state_root.path())
            .unwrap();
        assert!(registry.refusals().is_empty());
    }
}
