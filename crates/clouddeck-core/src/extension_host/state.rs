use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};

use super::manifest::Capability;

pub const STATE_FILE: &str = "state.json";
pub const CRASH_LIMIT: u32 = 2;

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub enabled: bool,
    /// A launch that began loading and never completed a call counts as a crash.
    #[serde(default)]
    pub launch_pending: bool,
    #[serde(default)]
    pub crashes: u32,
    /// Capabilities the user has granted; effective only if the manifest also declares them.
    #[serde(default)]
    pub granted: Vec<Capability>,
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
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

    /// The launch ended without a crash: loading failed or a call ran out of time.
    pub fn launch_ended(&mut self) {
        self.launch_pending = false;
    }

    /// A call returned, so the library ran without crashing.
    pub fn launch_completed(&mut self) {
        self.launch_pending = false;
        self.crashes = 0;
    }

    pub fn has_launch_history(&self) -> bool {
        self.launch_pending || self.crashes > 0
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
    pub fn open(dir: &Path) -> io::Result<Self> {
        let path = dir.join(STATE_FILE);
        let state = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => State::default(),
            Err(error) => return Err(error),
        };
        Ok(Self {
            path,
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
    let temp = path.with_extension("json.tmp");
    fs::write(
        &temp,
        serde_json::to_vec_pretty(state).map_err(io::Error::other)?,
    )?;
    fs::rename(temp, path)
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
    fn completed_call_clears_crash_history() {
        let mut state = State {
            launch_pending: true,
            crashes: 1,
            ..State::default()
        };
        state.launch_completed();
        assert_eq!(state.crashes, 0);
        assert!(!state.launch_pending);
    }

    #[test]
    fn state_round_trips_through_the_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::open(dir.path()).unwrap();
        store
            .update(|state| {
                state.enabled = true;
                state.grant(Capability::Secrets);
            })
            .unwrap();
        let reopened = StateStore::open(dir.path()).unwrap();
        assert!(reopened.read(|state| state.enabled));
        assert!(reopened.read(|state| state.is_granted(&Capability::Secrets)));
    }
}
