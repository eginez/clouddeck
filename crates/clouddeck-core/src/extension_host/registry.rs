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

use super::manifest::{MANIFEST_FILE, Manifest};
use super::remote::{Plugin, RemoteProvider};
use super::state::{CRASH_LIMIT, State, StateStore};

const CLOUDDECK_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub extension: String,
    pub message: String,
}

#[derive(Default)]
pub struct ProviderRegistry {
    providers: Vec<Arc<dyn Provider>>,
    provider_ids: BTreeSet<String>,
    refusals: Vec<Refusal>,
}

impl ProviderRegistry {
    pub fn with_builtins(demo: bool) -> Self {
        let mut registry = Self::default();
        for provider in builtin_providers(demo) {
            let id = provider.info().id;
            registry.provider_ids.insert(id);
            registry.providers.push(provider);
        }
        registry
    }

    pub fn providers(&self) -> &[Arc<dyn Provider>] {
        &self.providers
    }

    pub fn refusals(&self) -> &[Refusal] {
        &self.refusals
    }

    /// Reads each installed extension's manifest and state. No library is loaded here.
    pub fn discover(&mut self, root: &Path) -> io::Result<()> {
        if !root.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the plugins root must be an absolute path",
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
            .filter(|path| path.is_dir())
            .collect();
        dirs.sort();
        for dir in dirs {
            if let Err(message) = self.add_extension(&dir) {
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

    fn add_extension(&mut self, dir: &Path) -> Result<(), String> {
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
        let state =
            Arc::new(StateStore::open(dir).map_err(|error| format!("cannot read state: {error}"))?);
        if !state.read(|s| s.enabled) {
            return Ok(());
        }
        if state.read(State::crash_disabled) {
            return Err(format!(
                "disabled after crashing on its last {CRASH_LIMIT} launches"
            ));
        }
        self.reserve_provider_ids(&manifest)?;
        self.register_remote(dir, manifest, state);
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

    fn register_remote(&mut self, dir: &Path, manifest: Manifest, state: Arc<StateStore>) {
        let declared: Vec<(String, String)> = manifest
            .providers
            .iter()
            .map(|(id, decl)| (id.clone(), decl.name.clone()))
            .collect();
        let plugin = Arc::new(Plugin::new(dir.to_path_buf(), manifest, state));
        for (id, name) in declared {
            let provider = RemoteProvider::new(Arc::clone(&plugin), id, name);
            self.providers.push(Arc::new(provider));
        }
    }
}

/// Where installed extensions live, next to the rest of CloudDeck's configuration.
pub fn plugins_root() -> PathBuf {
    config_home().join("clouddeck/plugins")
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

    fn enable(path: &Path, change: impl FnOnce(&mut State)) {
        StateStore::open(path).unwrap().update(change).unwrap();
    }

    #[test]
    fn discovery_refuses_mismatched_api_and_names_the_rebuild() {
        let root = tempfile::tempdir().unwrap();
        let dir = install(root.path(), "old", &manifest_text("old", API_VERSION + 1));
        enable(&dir, |s| s.enabled = true);

        let mut registry = ProviderRegistry::default();
        registry.discover(root.path()).unwrap();

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
        install(root.path(), "quiet", &manifest_text("quiet", API_VERSION));

        let mut registry = ProviderRegistry::default();
        registry.discover(root.path()).unwrap();

        assert!(registry.providers().is_empty());
        assert!(registry.refusals().is_empty());
    }

    #[test]
    fn crash_disabled_extension_is_refused_at_discovery() {
        let root = tempfile::tempdir().unwrap();
        let dir = install(root.path(), "crashy", &manifest_text("crashy", API_VERSION));
        enable(&dir, |s| {
            s.enabled = true;
            s.crashes = CRASH_LIMIT;
        });

        let mut registry = ProviderRegistry::default();
        registry.discover(root.path()).unwrap();

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
        let dir = install(root.path(), "other", &manifest_text("toy", API_VERSION));
        enable(&dir, |s| s.enabled = true);

        let mut registry = ProviderRegistry::default();
        registry.discover(root.path()).unwrap();

        assert!(
            registry.refusals()[0]
                .message
                .contains("different folder name")
        );
    }

    #[test]
    fn missing_plugins_root_is_empty() {
        let root = tempfile::tempdir().unwrap();
        let mut registry = ProviderRegistry::default();
        registry.discover(&root.path().join("absent")).unwrap();
        assert!(registry.refusals().is_empty());
    }
}
