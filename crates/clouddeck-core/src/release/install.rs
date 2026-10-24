//! Installing a verified release into place.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use flate2::read::GzDecoder;

const UNSAFE_PATH: &str = "unsafe path in archive";

/// Extracts a gzipped tarball into an existing directory, refusing unsafe paths.
pub fn unpack(tar_gz: &[u8], dest: &Path) -> Result<(), String> {
    let mut archive = tar::Archive::new(GzDecoder::new(tar_gz));
    let entries = archive.entries().map_err(|e| e.to_string())?;
    for entry in entries {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let relative_ok = {
            let path = entry.path().map_err(|e| e.to_string())?;
            is_safe_relative(&path)
        };
        if !relative_ok {
            return Err(UNSAFE_PATH.into());
        }
        if !entry.unpack_in(dest).map_err(|e| e.to_string())? {
            return Err(UNSAFE_PATH.into());
        }
    }
    Ok(())
}

/// Returns the .app bundle that contains the given executable, if it lives in one.
pub fn bundle_root_from_exe(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    if macos.file_name()? != "MacOS" {
        return None;
    }
    let contents = macos.parent()?;
    if contents.file_name()? != "Contents" {
        return None;
    }
    let bundle = contents.parent()?;
    if !bundle.file_name()?.to_str()?.ends_with(".app") {
        return None;
    }
    Some(bundle.to_path_buf())
}

/// Replaces an installed .app bundle with the one in the archive, rolling back on failure.
pub fn install_macos(tar_gz: &[u8], bundle: &Path) -> Result<(), String> {
    let parent = bundle
        .parent()
        .ok_or_else(|| "bundle has no parent directory".to_string())?;
    let staging = StagingDir::create(parent)?;
    unpack(tar_gz, staging.path())?;
    let new = staging.path().join("CloudDeck.app");
    if !new.exists() {
        return Err("archive has no CloudDeck.app".into());
    }
    swap(&new, bundle, &sibling_with_suffix(bundle, ".old"))
}

/// Replaces the running executable with the one in the archive, rolling back on failure.
pub fn install_linux(tar_gz: &[u8], exe: &Path) -> Result<(), String> {
    let parent = exe
        .parent()
        .ok_or_else(|| "executable has no parent directory".to_string())?;
    let staging = StagingDir::create(parent)?;
    unpack(tar_gz, staging.path())?;
    let new = staging.path().join("clouddeck");
    if !new.is_file() {
        return Err("archive has no clouddeck binary".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&new, fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("could not make new binary executable: {e}"))?;
    }
    swap(&new, exe, &sibling_with_suffix(exe, ".old"))
}

fn is_safe_relative(path: &Path) -> bool {
    path.components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

fn sibling_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

fn swap(new: &Path, live: &Path, old: &Path) -> Result<(), String> {
    remove_existing(old).map_err(|e| format!("could not clear {}: {e}", old.display()))?;
    fs::rename(live, old).map_err(|e| format!("could not move {} aside: {e}", live.display()))?;
    if let Err(e) = fs::rename(new, live) {
        return match fs::rename(old, live) {
            Ok(()) => Err(format!("could not put new build in place: {e}")),
            Err(re) => Err(format!(
                "could not put new build in place: {e}; rollback failed ({re}), previous build left at {}",
                old.display()
            )),
        };
    }
    let _ = remove_existing(old);
    Ok(())
}

fn remove_existing(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

struct StagingDir(PathBuf);

impl StagingDir {
    fn create(parent: &Path) -> Result<Self, String> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = parent.join(format!(
            ".clouddeck-install-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir(&path).map_err(|e| format!("could not create staging directory: {e}"))?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for StagingDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;

    fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
        for (path, data) in entries {
            let mut header = tar::Header::new_gnu();
            header.as_old_mut().name[..path.len()].copy_from_slice(path.as_bytes());
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, *data).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn only_entries(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn unpack_writes_nested_file() {
        let dir = tempfile::tempdir().unwrap();
        let tgz = archive(&[("a/b.txt", b"hello")]);
        unpack(&tgz, dir.path()).unwrap();
        assert_eq!(fs::read_to_string(dir.path().join("a/b.txt")).unwrap(), "hello");
    }

    #[test]
    fn unpack_rejects_parent_dir_escape() {
        let root = tempfile::tempdir().unwrap();
        let dest = root.path().join("dest");
        fs::create_dir(&dest).unwrap();
        let tgz = archive(&[("../evil", b"x")]);
        assert_eq!(unpack(&tgz, &dest), Err("unsafe path in archive".to_string()));
        assert!(!root.path().join("evil").exists());
    }

    #[test]
    fn bundle_root_found_for_app_executable() {
        assert_eq!(
            bundle_root_from_exe(Path::new("/Applications/CloudDeck.app/Contents/MacOS/clouddeck")),
            Some(PathBuf::from("/Applications/CloudDeck.app"))
        );
    }

    #[test]
    fn bundle_root_none_outside_bundle() {
        assert_eq!(bundle_root_from_exe(Path::new("/usr/local/bin/clouddeck")), None);
    }

    #[test]
    fn install_macos_replaces_bundle() {
        let root = tempfile::tempdir().unwrap();
        let apps = root.path().join("apps");
        fs::create_dir(&apps).unwrap();
        let bundle = apps.join("CloudDeck.app");
        fs::create_dir_all(bundle.join("Contents")).unwrap();
        fs::write(bundle.join("Contents/Info.plist"), "old").unwrap();

        let tgz = archive(&[("CloudDeck.app/Contents/Info.plist", b"new")]);
        install_macos(&tgz, &bundle).unwrap();

        assert_eq!(fs::read_to_string(bundle.join("Contents/Info.plist")).unwrap(), "new");
        assert_eq!(only_entries(&apps), vec!["CloudDeck.app".to_string()]);
    }

    #[test]
    fn install_macos_without_bundle_in_archive_leaves_old_untouched() {
        let root = tempfile::tempdir().unwrap();
        let apps = root.path().join("apps");
        fs::create_dir(&apps).unwrap();
        let bundle = apps.join("CloudDeck.app");
        fs::create_dir_all(bundle.join("Contents")).unwrap();
        fs::write(bundle.join("Contents/Info.plist"), "old").unwrap();

        let tgz = archive(&[("Other/file.txt", b"x")]);
        assert_eq!(
            install_macos(&tgz, &bundle),
            Err("archive has no CloudDeck.app".to_string())
        );

        assert_eq!(fs::read_to_string(bundle.join("Contents/Info.plist")).unwrap(), "old");
        assert_eq!(only_entries(&apps), vec!["CloudDeck.app".to_string()]);
    }

    #[test]
    fn install_linux_replaces_executable_content() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("clouddeck");
        fs::write(&exe, "old").unwrap();

        let tgz = archive(&[("clouddeck", b"new")]);
        install_linux(&tgz, &exe).unwrap();

        assert_eq!(fs::read_to_string(&exe).unwrap(), "new");
    }

    #[cfg(unix)]
    #[test]
    fn install_linux_sets_executable_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("clouddeck");
        fs::write(&exe, "old").unwrap();

        let tgz = archive(&[("clouddeck", b"new")]);
        install_linux(&tgz, &exe).unwrap();

        let mode = fs::metadata(&exe).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o755);
    }

    #[test]
    fn install_linux_without_binary_leaves_old_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("clouddeck");
        fs::write(&exe, "old").unwrap();

        let tgz = archive(&[("README.txt", b"x")]);
        assert_eq!(
            install_linux(&tgz, &exe),
            Err("archive has no clouddeck binary".to_string())
        );

        assert_eq!(fs::read_to_string(&exe).unwrap(), "old");
        assert_eq!(only_entries(dir.path()), vec!["clouddeck".to_string()]);
    }

    #[test]
    fn install_linux_tolerates_stale_old_file() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("clouddeck");
        fs::write(&exe, "old").unwrap();
        fs::write(dir.path().join("clouddeck.old"), "stale").unwrap();

        let tgz = archive(&[("clouddeck", b"new")]);
        install_linux(&tgz, &exe).unwrap();

        assert_eq!(fs::read_to_string(&exe).unwrap(), "new");
        assert_eq!(only_entries(dir.path()), vec!["clouddeck".to_string()]);
    }
}
