//! Installing a verified release into place.

use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use flate2::read::GzDecoder;

const UNSAFE_PATH: &str = "unsafe path in archive";
const UNSUPPORTED_ENTRY: &str = "unsupported entry type in archive";
const TOO_MANY_ENTRIES: &str = "archive has too many entries";
const FILE_TOO_LARGE: &str = "archive file is larger than the size limit";
const TOTAL_TOO_LARGE: &str = "archive expands past the size limit";
const EXTENSION_TOO_LARGE: &str = "archive extension header is larger than the size limit";
const PAX_SIZE_OVERRIDE: &str = "archive pax header overrides entry size";

const BLOCK: u64 = 512;
const MAX_EXTENSION_HEADER_BYTES: u64 = 64 * 1024;
const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;

pub struct UnpackLimits {
    pub max_total_bytes: u64,
    pub max_entries: u64,
    pub max_file_bytes: u64,
}

pub const APP_UNPACK_LIMITS: UnpackLimits = UnpackLimits {
    max_total_bytes: GIB,
    max_entries: 20_000,
    max_file_bytes: 600 * MIB,
};
pub const EXTENSION_UNPACK_LIMITS: UnpackLimits = UnpackLimits {
    max_total_bytes: GIB,
    max_entries: 20_000,
    max_file_bytes: 600 * MIB,
};

/// Extracts a gzipped tarball into an existing directory. Only regular files and
/// directories are accepted; links and special files are refused. On error, `dest`
/// may hold partial output, so callers pass a scratch directory.
pub fn unpack(tar_gz: &[u8], dest: &Path, limits: UnpackLimits) -> Result<(), String> {
    let decoded = ExtensionGuard::new(Budget {
        inner: GzDecoder::new(tar_gz),
        remaining: limits.max_total_bytes,
    });
    let mut archive = tar::Archive::new(decoded);
    let entries = archive.entries().map_err(|e| e.to_string())?;
    for (index, entry) in entries.enumerate() {
        if index as u64 >= limits.max_entries {
            return Err(TOO_MANY_ENTRIES.into());
        }
        let mut entry = entry.map_err(|e| e.to_string())?;
        let entry_type = entry.header().entry_type();
        if !entry_type.is_file() && !entry_type.is_dir() {
            return Err(UNSUPPORTED_ENTRY.into());
        }
        if entry_type.is_file() && entry.size() > limits.max_file_bytes {
            return Err(FILE_TOO_LARGE.into());
        }
        let executable = entry.header().mode().map_err(|e| e.to_string())? & 0o111 != 0;
        let relative = entry.path().map_err(|e| e.to_string())?.into_owned();
        if !is_safe_relative(&relative) {
            return Err(UNSAFE_PATH.into());
        }
        if !entry.unpack_in(dest).map_err(|e| e.to_string())? {
            return Err(UNSAFE_PATH.into());
        }
        if entry_type.is_file() {
            set_file_mode(&dest.join(&relative), executable)?;
        }
    }
    Ok(())
}

struct Budget<R> {
    inner: R,
    remaining: u64,
}

impl<R: Read> Read for Budget<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let room = self.remaining.saturating_add(1).min(buf.len() as u64) as usize;
        let n = self.inner.read(&mut buf[..room])?;
        self.remaining = self
            .remaining
            .checked_sub(n as u64)
            .ok_or_else(|| io::Error::other(TOTAL_TOO_LARGE))?;
        Ok(n)
    }
}

/// The tar crate buffers extension-header bodies (GNU long names, PAX records) without
/// a size check. This reader follows the 512-byte block framing of the bytes it passes
/// through, so an oversized extension body is refused before it is buffered.
struct ExtensionGuard<R> {
    inner: R,
    header: [u8; BLOCK as usize],
    header_len: usize,
    body_left: u64,
    body_kind: Option<u8>,
    body_declared: u64,
    body: Vec<u8>,
}

impl<R> ExtensionGuard<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            header: [0; BLOCK as usize],
            header_len: 0,
            body_left: 0,
            body_kind: None,
            body_declared: 0,
            body: Vec::new(),
        }
    }

    fn observe(&mut self, mut bytes: &[u8]) -> io::Result<()> {
        while !bytes.is_empty() {
            if self.body_left > 0 {
                let take = self.body_left.min(bytes.len() as u64) as usize;
                if self.body_kind.is_some() {
                    self.body.extend_from_slice(&bytes[..take]);
                }
                self.body_left -= take as u64;
                bytes = &bytes[take..];
                if self.body_left == 0 {
                    self.finish_body()?;
                }
                continue;
            }
            let take = (BLOCK as usize - self.header_len).min(bytes.len());
            self.header[self.header_len..self.header_len + take].copy_from_slice(&bytes[..take]);
            self.header_len += take;
            bytes = &bytes[take..];
            if self.header_len == BLOCK as usize {
                self.header_len = 0;
                self.start_entry()?;
            }
        }
        Ok(())
    }

    fn start_entry(&mut self) -> io::Result<()> {
        let size = parse_header_size(&self.header[124..136])?;
        let kind = self.header[156];
        let is_extension = matches!(kind, b'L' | b'K' | b'x' | b'g');
        if is_extension && size > MAX_EXTENSION_HEADER_BYTES {
            return Err(io::Error::other(EXTENSION_TOO_LARGE));
        }
        self.body_kind = is_extension.then_some(kind);
        self.body_declared = size;
        self.body.clear();
        self.body_left = size.div_ceil(BLOCK).saturating_mul(BLOCK);
        if self.body_left == 0 && self.body_kind.is_some() {
            self.finish_body()?;
        }
        Ok(())
    }

    fn finish_body(&mut self) -> io::Result<()> {
        if matches!(self.body_kind, Some(b'x' | b'g')) {
            let declared = &self.body[..self.body_declared as usize];
            if declared.windows(5).any(|w| w == b"size=") {
                return Err(io::Error::other(PAX_SIZE_OVERRIDE));
            }
        }
        self.body_kind = None;
        self.body.clear();
        Ok(())
    }
}

impl<R: Read> Read for ExtensionGuard<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.observe(&buf[..n])?;
        Ok(n)
    }
}

fn parse_header_size(field: &[u8]) -> io::Result<u64> {
    if field[0] & 0x80 != 0 {
        let mut value = u64::from(field[0] & 0x7f);
        for &byte in &field[1..] {
            if value > u64::MAX >> 8 {
                return Err(io::Error::other("archive entry size is out of range"));
            }
            value = (value << 8) | u64::from(byte);
        }
        return Ok(value);
    }
    let invalid = || io::Error::other("invalid archive entry size");
    let end = field
        .iter()
        .position(|&b| b == 0 || b == b' ')
        .unwrap_or(field.len());
    if field[end..].iter().any(|&b| b != 0 && b != b' ') {
        return Err(invalid());
    }
    field[..end].iter().try_fold(0u64, |value, &digit| {
        if !(b'0'..=b'7').contains(&digit) {
            return Err(invalid());
        }
        value
            .checked_mul(8)
            .and_then(|v| v.checked_add(u64::from(digit - b'0')))
            .ok_or_else(invalid)
    })
}

#[cfg(unix)]
fn set_file_mode(path: &Path, executable: bool) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mode = if executable { 0o755 } else { 0o644 };
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|e| format!("could not set mode on {}: {e}", path.display()))
}

#[cfg(not(unix))]
fn set_file_mode(_path: &Path, _executable: bool) -> Result<(), String> {
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
    unpack(tar_gz, staging.path(), APP_UNPACK_LIMITS)?;
    let new = staging.path().join("CloudDeck.app");
    if !is_real_dir(&new) {
        return Err("archive has no CloudDeck.app".into());
    }
    swap_bundle(&new, bundle)
}

/// Replaces the running executable with the one in the archive, rolling back on failure.
pub fn install_linux(tar_gz: &[u8], exe: &Path) -> Result<(), String> {
    let parent = exe
        .parent()
        .ok_or_else(|| "executable has no parent directory".to_string())?;
    let staging = StagingDir::create(parent)?;
    unpack(tar_gz, staging.path(), APP_UNPACK_LIMITS)?;
    let new = staging.path().join("clouddeck");
    if !is_real_file(&new) {
        return Err("archive has no clouddeck binary".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&new, fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("could not make new binary executable: {e}"))?;
    }
    fs::rename(&new, exe).map_err(|e| format!("could not put new build in place: {e}"))
}

fn is_real_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_file())
}

fn is_real_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_dir())
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

#[cfg(target_os = "macos")]
fn swap_bundle(new: &Path, live: &Path) -> Result<(), String> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let from = CString::new(new.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    let to = CString::new(live.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    // SAFETY: both pointers are valid NUL-terminated paths for the duration of the call.
    let rc = unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), libc::RENAME_SWAP) };
    if rc != 0 {
        return Err(format!(
            "could not swap new build into {}: {}",
            live.display(),
            io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn swap_bundle(new: &Path, live: &Path) -> Result<(), String> {
    swap(new, live, &sibling_with_suffix(live, ".old"))
}

#[cfg(not(target_os = "macos"))]
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

#[cfg(not(target_os = "macos"))]
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
        let path = parent.join(format!(".clouddeck-install-{}-{nanos}", std::process::id()));
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
        unpack(&tgz, dir.path(), APP_UNPACK_LIMITS).unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("a/b.txt")).unwrap(),
            "hello"
        );
    }

    #[test]
    fn unpack_rejects_parent_dir_escape() {
        let root = tempfile::tempdir().unwrap();
        let dest = root.path().join("dest");
        fs::create_dir(&dest).unwrap();
        let tgz = archive(&[("../evil", b"x")]);
        assert_eq!(
            unpack(&tgz, &dest, APP_UNPACK_LIMITS),
            Err("unsafe path in archive".to_string())
        );
        assert!(!root.path().join("evil").exists());
    }

    fn archive_with_link(path: &str, target: &str, link_type: tar::EntryType) -> Vec<u8> {
        let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(link_type);
        header.set_mode(0o777);
        header.set_size(0);
        builder.append_link(&mut header, path, target).unwrap();
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn archive_with_path(path: &str) -> Vec<u8> {
        let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o644);
        header.set_size(1);
        builder.append_data(&mut header, path, &b"x"[..]).unwrap();
        builder.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn unpack_accepts_long_path_under_extension_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = format!("{}f.txt", "d/".repeat(200));
        unpack(&archive_with_path(&path), dir.path(), APP_UNPACK_LIMITS).unwrap();
        assert!(dir.path().join(&path).is_file());
    }

    #[test]
    fn unpack_refuses_extension_header_over_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = format!("{}f.txt", "d/".repeat(40_000));
        assert_eq!(
            unpack(&archive_with_path(&path), dir.path(), APP_UNPACK_LIMITS),
            Err(EXTENSION_TOO_LARGE.to_string())
        );
        assert!(only_entries(dir.path()).is_empty());
    }

    #[test]
    fn unpack_refuses_pax_size_override() {
        let dir = tempfile::tempdir().unwrap();
        let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
        builder
            .append_pax_extensions([("size", b"1".as_slice())])
            .unwrap();
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o644);
        header.set_size(1);
        builder.append_data(&mut header, "f", &b"x"[..]).unwrap();
        let tgz = builder.into_inner().unwrap().finish().unwrap();
        assert_eq!(
            unpack(&tgz, dir.path(), APP_UNPACK_LIMITS),
            Err(PAX_SIZE_OVERRIDE.to_string())
        );
    }

    #[test]
    fn unpack_rejects_symlink_entry() {
        let root = tempfile::tempdir().unwrap();
        let dest = root.path().join("dest");
        fs::create_dir(&dest).unwrap();
        let tgz = archive_with_link("clouddeck", "../../outside", tar::EntryType::Symlink);
        assert_eq!(
            unpack(&tgz, &dest, APP_UNPACK_LIMITS),
            Err(UNSUPPORTED_ENTRY.to_string())
        );
        assert!(only_entries(&dest).is_empty());
    }

    #[test]
    fn unpack_rejects_hard_link_entry() {
        let root = tempfile::tempdir().unwrap();
        let dest = root.path().join("dest");
        fs::create_dir(&dest).unwrap();
        let tgz = archive_with_link("clouddeck", "/etc/hostname", tar::EntryType::Link);
        assert_eq!(
            unpack(&tgz, &dest, APP_UNPACK_LIMITS),
            Err(UNSUPPORTED_ENTRY.to_string())
        );
        assert!(only_entries(&dest).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn install_linux_refuses_symlink_binary_and_leaves_old_untouched() {
        let root = tempfile::tempdir().unwrap();
        let outside = root.path().join("outside");
        fs::write(&outside, "target").unwrap();
        let dir = root.path().join("bin");
        fs::create_dir(&dir).unwrap();
        let exe = dir.join("clouddeck");
        fs::write(&exe, "old").unwrap();

        let tgz = archive_with_link(
            "clouddeck",
            outside.to_str().unwrap(),
            tar::EntryType::Symlink,
        );
        assert!(install_linux(&tgz, &exe).is_err());

        assert_eq!(fs::read_to_string(&exe).unwrap(), "old");
        assert_eq!(fs::read_to_string(&outside).unwrap(), "target");
    }

    #[cfg(unix)]
    #[test]
    fn unpack_normalizes_file_modes() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
        for (name, mode) in [("plain", 0o666), ("program", 0o777)] {
            let mut header = tar::Header::new_gnu();
            header.set_mode(mode);
            header.set_size(1);
            header.set_cksum();
            builder.append_data(&mut header, name, &b"x"[..]).unwrap();
        }
        let tgz = builder.into_inner().unwrap().finish().unwrap();
        unpack(&tgz, dir.path(), APP_UNPACK_LIMITS).unwrap();

        let mode = |name: &str| {
            fs::metadata(dir.path().join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o777
        };
        assert_eq!(mode("plain"), 0o644);
        assert_eq!(mode("program"), 0o755);
    }

    #[test]
    fn bundle_root_found_for_app_executable() {
        assert_eq!(
            bundle_root_from_exe(Path::new(
                "/Applications/CloudDeck.app/Contents/MacOS/clouddeck"
            )),
            Some(PathBuf::from("/Applications/CloudDeck.app"))
        );
    }

    #[test]
    fn bundle_root_none_outside_bundle() {
        assert_eq!(
            bundle_root_from_exe(Path::new("/usr/local/bin/clouddeck")),
            None
        );
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

        assert_eq!(
            fs::read_to_string(bundle.join("Contents/Info.plist")).unwrap(),
            "new"
        );
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

        assert_eq!(
            fs::read_to_string(bundle.join("Contents/Info.plist")).unwrap(),
            "old"
        );
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
    fn install_linux_never_moves_live_binary_aside() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("clouddeck");
        fs::write(&exe, "old").unwrap();
        fs::write(dir.path().join("clouddeck.old"), "stale").unwrap();

        let tgz = archive(&[("clouddeck", b"new")]);
        install_linux(&tgz, &exe).unwrap();

        assert_eq!(fs::read_to_string(&exe).unwrap(), "new");
        assert_eq!(
            fs::read_to_string(dir.path().join("clouddeck.old")).unwrap(),
            "stale"
        );
        assert_eq!(
            only_entries(dir.path()),
            vec!["clouddeck".to_string(), "clouddeck.old".to_string()]
        );
    }

    const TEST_LIMITS: UnpackLimits = UnpackLimits {
        max_total_bytes: 1 << 20,
        max_entries: 100,
        max_file_bytes: 1 << 20,
    };

    fn dir_bytes(dir: &Path) -> u64 {
        fs::read_dir(dir)
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dir_bytes(&path)
                } else {
                    fs::metadata(&path).unwrap().len()
                }
            })
            .sum()
    }

    fn release_archive(dirs: &[&str], files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
        for dir in dirs {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Directory);
            header.set_mode(0o755);
            header.set_size(0);
            builder.append_data(&mut header, dir, io::empty()).unwrap();
        }
        for (path, data) in files {
            let mut header = tar::Header::new_gnu();
            header.set_mode(0o755);
            header.set_size(data.len() as u64);
            builder.append_data(&mut header, path, *data).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn gzip_bomb_is_refused_without_writing_past_total_cap() {
        let dir = tempfile::tempdir().unwrap();
        let zeros = vec![0u8; 16 << 20];
        let tgz = archive(&[("big.bin", &zeros)]);
        assert!(tgz.len() < 1 << 20);
        let limits = UnpackLimits {
            max_file_bytes: 16 << 20,
            ..TEST_LIMITS
        };
        assert!(unpack(&tgz, dir.path(), limits).is_err());
        assert!(dir_bytes(dir.path()) <= TEST_LIMITS.max_total_bytes);
    }

    #[test]
    fn archive_with_too_many_entries_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let tgz = archive(&[("a", b"1"), ("b", b"2"), ("c", b"3")]);
        let limits = UnpackLimits {
            max_entries: 2,
            ..TEST_LIMITS
        };
        assert_eq!(
            unpack(&tgz, dir.path(), limits),
            Err(TOO_MANY_ENTRIES.to_string())
        );
    }

    #[test]
    fn archive_at_entry_and_total_caps_is_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let tgz = archive(&[("a", b"1"), ("b", b"2")]);
        let limits = UnpackLimits {
            max_entries: 2,
            max_total_bytes: decompressed_len(&tgz),
            ..TEST_LIMITS
        };
        unpack(&tgz, dir.path(), limits).unwrap();
    }

    fn decompressed_len(tgz: &[u8]) -> u64 {
        let mut plain = Vec::new();
        GzDecoder::new(tgz).read_to_end(&mut plain).unwrap();
        plain.len() as u64
    }

    #[test]
    fn file_over_max_file_bytes_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let big = vec![7u8; 2048];
        let tgz = archive(&[("small", b"ok"), ("big", &big)]);
        let limits = UnpackLimits {
            max_file_bytes: 1024,
            ..TEST_LIMITS
        };
        assert_eq!(
            unpack(&tgz, dir.path(), limits),
            Err(FILE_TOO_LARGE.to_string())
        );
        assert!(!dir.path().join("big").exists());
    }

    #[test]
    fn macos_release_shaped_archive_unpacks_with_app_limits() {
        let dir = tempfile::tempdir().unwrap();
        let binary = vec![1u8; 4 << 20];
        let tgz = release_archive(
            &[
                "CloudDeck.app/",
                "CloudDeck.app/Contents/",
                "CloudDeck.app/Contents/MacOS/",
                "CloudDeck.app/Contents/Resources/",
            ],
            &[
                ("CloudDeck.app/Contents/Info.plist", b"<plist/>"),
                ("CloudDeck.app/Contents/MacOS/clouddeck", &binary),
                ("CloudDeck.app/Contents/Resources/AppIcon.icns", b"icns"),
            ],
        );
        unpack(&tgz, dir.path(), APP_UNPACK_LIMITS).unwrap();
        assert!(
            dir.path()
                .join("CloudDeck.app/Contents/MacOS/clouddeck")
                .is_file()
        );
    }

    #[test]
    fn linux_release_shaped_archive_unpacks_with_app_limits() {
        let dir = tempfile::tempdir().unwrap();
        let binary = vec![2u8; 4 << 20];
        let icon = vec![3u8; 64 << 10];
        let sizes = [16, 32, 128, 256, 512, 1024];
        let icon_dirs: Vec<String> = sizes
            .iter()
            .map(|s| format!("./share/icons/hicolor/{s}x{s}/apps/"))
            .collect();
        let icon_files: Vec<String> = sizes
            .iter()
            .map(|s| format!("./share/icons/hicolor/{s}x{s}/apps/clouddeck.png"))
            .collect();
        let mut dirs = vec!["./", "./share/", "./share/icons/", "./share/icons/hicolor/"];
        dirs.extend(icon_dirs.iter().map(String::as_str));
        let mut files: Vec<(&str, &[u8])> = vec![
            ("./clouddeck", binary.as_slice()),
            ("./clouddeck.desktop", &b"[Desktop Entry]"[..]),
            ("./clouddeck.png", icon.as_slice()),
            ("./README.txt", &b"readme"[..]),
        ];
        files.extend(icon_files.iter().map(|p| (p.as_str(), icon.as_slice())));
        let tgz = release_archive(&dirs, &files);
        unpack(&tgz, dir.path(), APP_UNPACK_LIMITS).unwrap();
        assert!(dir.path().join("clouddeck").is_file());
        assert!(
            dir.path()
                .join("share/icons/hicolor/1024x1024/apps/clouddeck.png")
                .is_file()
        );
    }
}
