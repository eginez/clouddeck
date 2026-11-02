#![cfg(unix)]

mod common;

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;

use clouddeck_core::extension_host::{ProviderRegistry, StateStore, state_path};
use clouddeck_core::{Call, ProviderError};
use common::{LIBRARY, NoHost, built_library, install, write_manifest};

fn connect(root: &Path, state_root: &Path) -> Result<(), ProviderError> {
    let state_file = state_path(state_root, "toy");
    StateStore::open(&state_file)
        .unwrap()
        .update(|state| state.enabled = true)
        .unwrap();
    let mut registry = ProviderRegistry::default();
    registry.discover(root, state_root).unwrap();
    assert!(registry.refusals().is_empty(), "{:?}", registry.refusals());
    let host = NoHost;
    let cx = Call {
        deadline: None,
        host: &host,
    };
    registry.providers()[0].connections(&cx).map(drop)
}

fn refusal(root: &Path, state_root: &Path) -> String {
    connect(root, state_root)
        .expect_err("the library must be refused before it is loaded")
        .message
}

fn set_mode(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
fn library_in_world_writable_folder_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let state_root = tempfile::tempdir().unwrap();
    install(root.path());
    let folder = root.path().join("toy");
    set_mode(&folder, 0o777);

    let message = refusal(root.path(), state_root.path());
    assert!(message.contains(&folder.display().to_string()), "{message}");
    assert!(message.contains("chmod go-w"), "{message}");
}

#[test]
fn group_writable_library_file_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let state_root = tempfile::tempdir().unwrap();
    install(root.path());
    set_mode(&root.path().join("toy").join(LIBRARY), 0o775);

    let message = refusal(root.path(), state_root.path());
    assert!(message.contains("chmod go-w"), "{message}");
}

#[test]
fn symlinked_library_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let state_root = tempfile::tempdir().unwrap();
    let folder = root.path().join("toy");
    fs::create_dir_all(&folder).unwrap();
    fs::copy(built_library(), folder.join("payload.so")).unwrap();
    symlink("payload.so", folder.join(LIBRARY)).unwrap();
    write_manifest(&folder);

    let message = refusal(root.path(), state_root.path());
    assert!(message.contains("symlink"), "{message}");
}

#[test]
fn library_owned_by_user_in_private_folders_loads() {
    let root = tempfile::tempdir().unwrap();
    let state_root = tempfile::tempdir().unwrap();
    install(root.path());
    set_mode(&root.path().join("toy"), 0o755);

    connect(root.path(), state_root.path()).unwrap();
}
