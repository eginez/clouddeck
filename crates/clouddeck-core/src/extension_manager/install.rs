use std::fs;
use std::path::{Path, PathBuf};

use crate::extension_host::{
    Capability, ProviderRegistry, Refusal, extension_state_root, extensions_root,
};
use crate::release::verify::RELEASE_PUBLIC_KEY;

use super::fetch::Plan;
use super::index::IndexError;
use super::source::{ExtensionSource, GithubExtensions, parse_repository, same_repository};
use super::stage::{Staging, copy_tree, io_error, read_manifest, remove_shipped_state, swap_in};
use super::store::{
    check_id, folders, index, installed_record, is_installed, record_install, require_installed,
    state,
};

/// Turned on by Esteban or the planner only after security review B (clouddeck-rs-7ah1) closes.
pub const EXTERNAL_EXTENSIONS_ALLOWED: bool = false;
const ENABLE_REFUSED: &str =
    "enabling external extensions is turned off until security review B (clouddeck-rs-7ah1) closes";
pub const MAX_EXTENSION_ASSET: u64 = 200 * 1024 * 1024;
const LOCK_FILE: &str = ".install.lock";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub id: String,
    pub version: String,
    pub repository: Option<String>,
    pub author_key: Option<String>,
    pub capabilities: Vec<Capability>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Installed(Installed),
    UpToDate { version: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub id: String,
    pub version: String,
    pub enabled: bool,
    pub dev: bool,
    pub crash_disabled: bool,
    pub repository: Option<String>,
    pub author_key: Option<String>,
}

pub(super) struct Dirs {
    pub(super) root: PathBuf,
    pub(super) state_root: PathBuf,
}

pub struct Manager {
    pub(super) dirs: Dirs,
    pub(super) source: Box<dyn ExtensionSource>,
    pub(super) release_key: String,
}

impl Manager {
    pub fn new(
        root: PathBuf,
        state_root: PathBuf,
        source: Box<dyn ExtensionSource>,
        release_key: String,
    ) -> Self {
        Self {
            dirs: Dirs { root, state_root },
            source,
            release_key,
        }
    }

    pub fn github() -> Self {
        Self::new(
            extensions_root(),
            extension_state_root(),
            Box::new(GithubExtensions::default()),
            RELEASE_PUBLIC_KEY.to_string(),
        )
    }

    fn lock(&self) -> Result<fs::File, String> {
        fs::create_dir_all(&self.dirs.state_root).map_err(io_error)?;
        let path = self.dirs.state_root.join(LOCK_FILE);
        let file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(io_error)?;
        match file.try_lock() {
            Ok(()) => Ok(file),
            Err(fs::TryLockError::WouldBlock) => Err(format!(
                "another extension operation is running; wait for it to finish ({} is held)",
                path.display()
            )),
            Err(fs::TryLockError::Error(error)) => Err(io_error(error)),
        }
    }

    pub fn install_from_index(&self, id: &str, accept: Option<&str>) -> Result<Outcome, String> {
        let _lock = self.lock()?;
        check_id(id)?;
        let index = index(&*self.source, &self.release_key).map_err(|error| error.to_string())?;
        let entry = index
            .find(id)
            .ok_or_else(|| format!("{id} is not in the extension index"))?;
        let (owner, repo) = parse_repository(&entry.repository)?;
        let repository = entry.repository.clone();
        let offered_key = entry.author_key.clone();
        self.install_release(Plan {
            owner,
            repo,
            repository,
            expected_id: Some(id.to_string()),
            offered_key: Some(offered_key),
            accept: accept.map(str::to_string),
            update: false,
            index: Some(index),
        })
    }

    pub fn install_from_url(
        &self,
        url: &str,
        trust_key: Option<&str>,
        accept: Option<&str>,
    ) -> Result<Outcome, String> {
        let _lock = self.lock()?;
        let (owner, repo) = parse_repository(url)?;
        let index = match index(&*self.source, &self.release_key) {
            Ok(index) => Some(index),
            Err(error @ IndexError::Unavailable(_)) if trust_key.is_none() => {
                return Err(format!(
                    "{url} cannot be checked against the extension index: {error}. \
                     Pass --trust-key <base64 author key> to install it without the index"
                ));
            }
            Err(IndexError::Unavailable(_)) => None,
            Err(error) => return Err(error.to_string()),
        };
        let listed = index.as_ref().and_then(|index| {
            index
                .extensions
                .iter()
                .find(|entry| same_repository(&entry.repository, &owner, &repo))
        });
        let offered = match (listed, trust_key) {
            (Some(entry), Some(key)) if key != entry.author_key => {
                return Err(format!(
                    "--trust-key does not match the index key {} for {url}",
                    entry.author_key
                ));
            }
            (Some(entry), _) => entry.author_key.clone(),
            (None, Some(key)) => key.to_string(),
            (None, None) => {
                return Err(format!(
                    "{url} is not in the extension index; pass --trust-key <base64 author key> to install it"
                ));
            }
        };
        self.install_release(Plan {
            owner,
            repo,
            repository: url.to_string(),
            expected_id: None,
            offered_key: Some(offered),
            accept: accept.map(str::to_string),
            update: false,
            index,
        })
    }

    pub fn update(&self, id: &str, accept: Option<&str>) -> Result<Outcome, String> {
        let _lock = self.lock()?;
        require_installed(&self.dirs, id)?;
        let (dev, repository) = state(&self.dirs, id)?.read(|s| (s.dev, s.repository.clone()));
        if dev {
            return Err(format!(
                "{id} is a dev extension and has no release to update from; reinstall it with dev <path>"
            ));
        }
        let repository = repository.ok_or_else(|| {
            format!("{id} has no recorded repository; reinstall it from the index")
        })?;
        let (owner, repo) = parse_repository(&repository)?;
        let index = match index(&*self.source, &self.release_key) {
            Ok(index) => Some(index),
            Err(IndexError::Unavailable(_)) => None,
            Err(error) => return Err(error.to_string()),
        };
        let offered = match index.as_ref().and_then(|index| index.find(id)) {
            Some(entry) if same_repository(&entry.repository, &owner, &repo) => {
                Some(entry.author_key.clone())
            }
            Some(entry) => {
                return Err(format!(
                    "the index lists {id} at {}, not {repository}",
                    entry.repository
                ));
            }
            None => None,
        };
        self.install_release(Plan {
            owner,
            repo,
            repository,
            expected_id: Some(id.to_string()),
            offered_key: offered,
            accept: accept.map(str::to_string),
            update: true,
            index,
        })
    }

    pub fn install_dev(&self, path: &Path) -> Result<Outcome, String> {
        let _lock = self.lock()?;
        let manifest = read_manifest(path)?;
        let id = manifest.id.clone();
        let existed = is_installed(&self.dirs, &id);
        if existed && state(&self.dirs, &id)?.read(|s| s.author_key.is_some() && !s.dev) {
            return Err(format!(
                "{id} is installed from a signed release; remove it before installing a dev copy"
            ));
        }
        let staging = Staging::create(&self.dirs.root, &id)?;
        copy_tree(path, staging.path())?;
        remove_shipped_state(staging.path())?;
        manifest.library_path(staging.path())?;
        swap_in(&self.dirs.root, &id, staging.path())?;
        record_install(&self.dirs, &id, !existed, |s| s.dev = true)?;
        Ok(Outcome::Installed(installed_record(&self.dirs, &manifest)?))
    }

    /// The hard gate: enabling an external extension is refused until security review B closes.
    pub fn enable(&self, id: &str) -> Result<(), String> {
        self.enable_with(id, EXTERNAL_EXTENSIONS_ALLOWED)
    }

    pub(super) fn enable_with(&self, id: &str, allowed: bool) -> Result<(), String> {
        if !allowed {
            return Err(ENABLE_REFUSED.to_string());
        }
        require_installed(&self.dirs, id)?;
        state(&self.dirs, id)?
            .update(|s| s.enabled = true)
            .map_err(io_error)
    }

    pub fn disable(&self, id: &str) -> Result<(), String> {
        require_installed(&self.dirs, id)?;
        state(&self.dirs, id)?
            .update(|s| s.enabled = false)
            .map_err(io_error)
    }

    /// Removes the folder and keeps the author key pin, so a reinstall cannot silently swap keys.
    pub fn remove(&self, id: &str) -> Result<(), String> {
        let _lock = self.lock()?;
        require_installed(&self.dirs, id)?;
        fs::remove_dir_all(self.dirs.root.join(id)).map_err(io_error)?;
        state(&self.dirs, id)?
            .update(|s| s.enabled = false)
            .map_err(io_error)
    }

    pub fn list(&self) -> Result<(Vec<Listing>, Vec<Refusal>), String> {
        let mut registry = ProviderRegistry::default();
        registry
            .discover(&self.dirs.root, &self.dirs.state_root)
            .map_err(io_error)?;
        let refusals = registry.refusals();
        let mut listings = Vec::new();
        for dir in folders(&self.dirs)? {
            let Ok(manifest) = read_manifest(&dir) else {
                continue;
            };
            if !dir.ends_with(&manifest.id) {
                continue;
            }
            let (enabled, dev, crash_disabled, repository, author_key) =
                state(&self.dirs, &manifest.id)?.read(|s| {
                    (
                        s.enabled,
                        s.dev,
                        s.crash_disabled(),
                        s.repository.clone(),
                        s.author_key.clone(),
                    )
                });
            listings.push(Listing {
                id: manifest.id,
                version: manifest.version,
                enabled,
                dev,
                crash_disabled,
                repository,
                author_key,
            });
        }
        Ok((listings, refusals))
    }
}
