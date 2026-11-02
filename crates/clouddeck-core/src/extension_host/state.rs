use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};

use super::manifest::Capability;

pub const CRASH_LIMIT: u32 = 2;

/// The host's state file for one extension, under the host-owned state root.
pub fn state_path(root: &Path, id: &str) -> PathBuf {
    root.join(format!("{id}.json"))
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub enabled: bool,
    /// Set while a loaded library runs; a session that ends without a clean shutdown leaves it set.
    #[serde(default)]
    pub launch_pending: bool,
    #[serde(default)]
    pub crashes: u32,
    /// Capabilities the user has granted; effective only if the manifest also declares them.
    #[serde(default)]
    pub granted: Vec<Capability>,
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
    /// The author public key pinned on first install; later installs and updates must match it.
    #[serde(default)]
    pub author_key: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    /// Installed from a local folder rather than a signed release.
    #[serde(default)]
    pub dev: bool,
}

impl State {
    /// Returns false when the extension has crashed on its last [`CRASH_LIMIT`] launches.
    pub fn begin_launch(&mut self) -> bool {
        if self.launch_pending {
            self.crashes += 1;
        }
        self.launch_pending = !self.crash_disabled();
        self.launch_pending
    }

    pub fn crash_disabled(&self) -> bool {
        self.crashes >= CRASH_LIMIT
    }

    /// The launch ended without a crash because loading failed.
    pub fn launch_ended(&mut self) {
        self.launch_pending = false;
    }

    /// The host shut down cleanly after the library was loaded, so the session did not crash.
    pub fn clean_shutdown(&mut self) {
        self.launch_pending = false;
        self.crashes = 0;
    }

    pub fn grant(&mut self, capability: Capability) {
        if !self.granted.contains(&capability) {
            self.granted.push(capability);
        }
    }

    pub fn is_granted(&self, capability: &Capability) -> bool {
        self.granted.contains(capability)
    }
}

pub struct StateStore {
    path: PathBuf,
    state: Mutex<State>,
}

impl StateStore {
    pub fn open(path: &Path) -> io::Result<Self> {
        let state = match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => State::default(),
            Err(error) => return Err(error),
        };
        Ok(Self {
            path: path.to_path_buf(),
            state: Mutex::new(state),
        })
    }

    pub fn read<T>(&self, read: impl FnOnce(&State) -> T) -> T {
        read(&lock(&self.state))
    }

    pub fn update<T>(&self, change: impl FnOnce(&mut State) -> T) -> io::Result<T> {
        let mut state = lock(&self.state);
        let value = change(&mut state);
        save(&self.path, &state)?;
        Ok(value)
    }
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

fn save(path: &Path, state: &State) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(state).map_err(io::Error::other)?;
    if let Some(dir) = path.parent() {
        create_private_dir(dir)?;
    }
    let temp = path.with_extension("json.tmp");
    create_private_file(&temp)?.write_all(&bytes)?;
    fs::rename(temp, path)
}

#[cfg(unix)]
fn create_private_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn create_private_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)
}

#[cfg(unix)]
fn create_private_file(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn create_private_file(path: &Path) -> io::Result<fs::File> {
    fs::File::create(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_crashed_launches_disable_the_extension() {
        let mut state = State::default();
        assert!(state.begin_launch());
        assert!(state.launch_pending);

        let mut state = State {
            launch_pending: true,
            ..State::default()
        };
        assert!(state.begin_launch());
        assert_eq!(state.crashes, 1);
        assert!(!state.begin_launch());
        assert!(state.crash_disabled());
    }

    #[test]
    fn served_session_without_clean_shutdown_counts_as_crash() {
        let mut state = State::default();
        assert!(state.begin_launch());
        state.begin_launch();
        assert_eq!(state.crashes, 1);
        assert!(state.launch_pending);
    }

    #[test]
    fn clean_shutdown_clears_crash_history() {
        let mut state = State {
            launch_pending: true,
            crashes: 1,
            ..State::default()
        };
        state.clean_shutdown();
        assert_eq!(state.crashes, 0);
        assert!(!state.launch_pending);
    }

    #[test]
    fn state_round_trips_through_the_store() {
        let dir = tempfile::tempdir().unwrap();
        let path = state_path(dir.path(), "toy");
        let store = StateStore::open(&path).unwrap();
        store
            .update(|state| {
                state.enabled = true;
                state.grant(Capability::Secrets);
            })
            .unwrap();
        let reopened = StateStore::open(&path).unwrap();
        assert!(reopened.read(|state| state.enabled));
        assert!(reopened.read(|state| state.is_granted(&Capability::Secrets)));
    }

    #[test]
    fn crash_count_persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = state_path(dir.path(), "toy");
        let store = StateStore::open(&path).unwrap();
        store.update(State::begin_launch).unwrap();
        store.update(State::begin_launch).unwrap();
        drop(store);

        let reopened = StateStore::open(&path).unwrap();
        assert_eq!(reopened.read(|state| state.crashes), 1);
        assert!(reopened.read(|state| state.launch_pending));
    }

    #[cfg(unix)]
    #[test]
    fn state_file_and_its_directory_are_private() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let state_dir = dir.path().join("extension-state");
        fs::create_dir_all(&state_dir).unwrap();
        fs::set_permissions(&state_dir, fs::Permissions::from_mode(0o755)).unwrap();
        let path = state_path(&state_dir, "toy");
        let stale = path.with_extension("json.tmp");
        fs::write(&stale, b"{}").unwrap();
        fs::set_permissions(&stale, fs::Permissions::from_mode(0o644)).unwrap();
        StateStore::open(&path)
            .unwrap()
            .update(|state| state.enabled = true)
            .unwrap();

        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&state_dir), 0o700);
        assert_eq!(mode(&path), 0o600);
    }
}
