use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use clouddeck_extension_api::API_VERSION;

use crate::extension_host::{MANIFEST_FILE, Manifest};

pub(super) fn read_manifest(dir: &Path) -> Result<Manifest, String> {
    let text = fs::read_to_string(dir.join(MANIFEST_FILE))
        .map_err(|error| format!("cannot read {MANIFEST_FILE}: {error}"))?;
    let manifest = Manifest::parse(&text)?;
    manifest.check_api(API_VERSION, env!("CARGO_PKG_VERSION"))?;
    Ok(manifest)
}

pub(super) fn remove_shipped_state(dir: &Path) -> Result<(), String> {
    match fs::remove_file(dir.join("state.json")) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(error)),
    }
}

pub(super) fn copy_tree(src: &Path, dst: &Path) -> Result<(), String> {
    for entry in fs::read_dir(src).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path).map_err(io_error)?;
        let target = dst.join(entry.file_name());
        if meta.file_type().is_symlink() {
            return Err(format!(
                "{} is a symbolic link; dev extensions must not contain symlinks",
                path.display()
            ));
        } else if meta.is_dir() {
            fs::create_dir(&target).map_err(io_error)?;
            copy_tree(&path, &target)?;
        } else {
            fs::copy(&path, &target).map_err(io_error)?;
        }
    }
    Ok(())
}

pub(super) fn swap_in(root: &Path, id: &str, staged: &Path) -> Result<(), String> {
    let live = root.join(id);
    if fs::symlink_metadata(&live).is_err() {
        return fs::rename(staged, &live)
            .map_err(|error| format!("could not install {id}: {error}"));
    }
    let old = root.join(format!(".old-{id}-{}", unique_suffix()));
    fs::rename(&live, &old)
        .map_err(|error| format!("could not move the installed {id} aside: {error}"))?;
    if let Err(error) = fs::rename(staged, &live) {
        let _ = fs::rename(&old, &live);
        return Err(format!("could not install {id}: {error}"));
    }
    let _ = fs::remove_dir_all(&old);
    Ok(())
}

/// A hidden staging folder beside the installed extensions; removed on drop unless renamed into place.
pub(super) struct Staging(PathBuf);

impl Staging {
    pub(super) fn create(root: &Path, label: &str) -> Result<Self, String> {
        fs::create_dir_all(root).map_err(io_error)?;
        let path = root.join(format!(
            ".tmp-{label}-{}-{}",
            std::process::id(),
            unique_suffix()
        ));
        fs::create_dir(&path)
            .map_err(|error| format!("cannot create a staging folder: {error}"))?;
        Ok(Self(path))
    }

    pub(super) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(super) fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0)
}

pub(super) fn io_error(error: io::Error) -> String {
    error.to_string()
}
