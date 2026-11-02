use std::fs;
use std::io;
use std::path::PathBuf;

use crate::extension_host::{Manifest, State, StateStore, is_valid_id, state_path};

use super::index::{Index, IndexError, verify_index};
use super::install::{Dirs, Installed};
use super::source::ExtensionSource;
use super::stage::io_error;

pub(super) fn record_install(
    dirs: &Dirs,
    id: &str,
    fresh: bool,
    change: impl FnOnce(&mut State),
) -> Result<(), String> {
    state(dirs, id)?
        .update(|s| {
            change(s);
            if fresh {
                s.enabled = false;
            }
        })
        .map_err(io_error)
}

pub(super) fn installed_record(dirs: &Dirs, manifest: &Manifest) -> Result<Installed, String> {
    let (repository, author_key) =
        state(dirs, &manifest.id)?.read(|s| (s.repository.clone(), s.author_key.clone()));
    Ok(Installed {
        id: manifest.id.clone(),
        version: manifest.version.clone(),
        repository,
        author_key,
        capabilities: manifest.capabilities.clone(),
    })
}

pub(super) fn index(source: &dyn ExtensionSource, release_key: &str) -> Result<Index, IndexError> {
    let (bytes, signature) = source.index()?;
    verify_index(&bytes, &signature, release_key).map_err(IndexError::Invalid)
}

pub(super) fn pinned_key(dirs: &Dirs, id: &str) -> Result<Option<String>, String> {
    Ok(state(dirs, id)?.read(|s| s.author_key.clone()))
}

pub(super) fn state(dirs: &Dirs, id: &str) -> Result<StateStore, String> {
    StateStore::open(&state_path(&dirs.state_root, id))
        .map_err(|error| format!("cannot read state for {id}: {error}"))
}

pub(super) fn is_installed(dirs: &Dirs, id: &str) -> bool {
    fs::symlink_metadata(dirs.root.join(id)).is_ok()
}

pub(super) fn require_installed(dirs: &Dirs, id: &str) -> Result<(), String> {
    check_id(id)?;
    if is_installed(dirs, id) {
        Ok(())
    } else {
        Err(format!("{id} is not installed"))
    }
}

pub(super) fn folders(dirs: &Dirs) -> Result<Vec<PathBuf>, String> {
    let entries = match fs::read_dir(&dirs.root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io_error(error)),
    };
    let mut found: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .is_some_and(|name| !name.to_string_lossy().starts_with('.'))
        })
        .collect();
    found.sort();
    Ok(found)
}

pub(super) fn check_id(id: &str) -> Result<(), String> {
    if is_valid_id(id) {
        Ok(())
    } else {
        Err(format!("{id:?} is not a valid extension id"))
    }
}
