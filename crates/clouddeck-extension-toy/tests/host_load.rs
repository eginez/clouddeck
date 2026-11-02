use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use clouddeck_core::extension_host::{ProviderRegistry, StateStore, state_path};
use clouddeck_core::{Call, ConnectionId, Host, LogLevel, ProviderError};

struct NoHost;

impl Host for NoHost {
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

const LIBRARY: &str = if cfg!(target_os = "macos") {
    "libclouddeck_extension_toy.dylib"
} else {
    "libclouddeck_extension_toy.so"
};

/// `cargo test` builds only the rlib, so the cdylib is built here into the same target directory.
fn built_library() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let profile_dir = exe.parent().unwrap().parent().unwrap().to_path_buf();
    let mut build = Command::new(env!("CARGO"));
    build.args(["build", "--lib", "-p", "clouddeck-extension-toy"]);
    if profile_dir.file_name() == Some("release".as_ref()) {
        build.arg("--release");
    }
    assert!(
        build.status().unwrap().success(),
        "building the toy cdylib failed"
    );
    profile_dir.join(LIBRARY)
}

fn install(root: &Path) {
    let dir = root.join("toy");
    fs::create_dir_all(&dir).unwrap();
    fs::copy(built_library(), dir.join(LIBRARY)).unwrap();
    fs::write(
        dir.join("extension.toml"),
        format!(
            "id = \"toy\"\nname = \"Toy\"\nversion = \"0.1.0\"\nschema_version = 1\napi_version = 1\nlibrary = \"{LIBRARY}\"\n\n[providers.toy]\nname = \"Toy\"\n"
        ),
    )
    .unwrap();
}

// One test: the extension's host table is process-wide, so the library can be initialised once.
#[test]
fn installed_extension_loads_on_first_use_and_serves_calls() {
    let root = tempfile::tempdir().unwrap();
    let state_root = tempfile::tempdir().unwrap();
    let state_file = state_path(state_root.path(), "toy");
    install(root.path());
    StateStore::open(&state_file)
        .unwrap()
        .update(|state| state.enabled = true)
        .unwrap();

    let mut registry = ProviderRegistry::default();
    registry.discover(root.path(), state_root.path()).unwrap();
    assert!(registry.refusals().is_empty(), "{:?}", registry.refusals());
    let provider = &registry.providers()[0];

    let store = StateStore::open(&state_file).unwrap();
    assert!(
        !store.read(|state| state.launch_pending),
        "discovery must not load"
    );

    let host = NoHost;
    let cx = Call {
        deadline: None,
        host: &host,
    };
    let connections = provider.connections(&cx).unwrap();
    assert_eq!(connections[0].id, ConnectionId::from("default"));
    assert_eq!(provider.info().id, "toy");

    let store = StateStore::open(&state_file).unwrap();
    assert!(store.read(|state| state.launch_pending));
    assert_eq!(store.read(|state| state.crashes), 0);

    registry.shutdown();
    let store = StateStore::open(&state_file).unwrap();
    assert!(!store.read(|state| state.launch_pending));
    assert_eq!(store.read(|state| state.crashes), 0);
}
