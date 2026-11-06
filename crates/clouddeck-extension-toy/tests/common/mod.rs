use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use clouddeck_core::{Host, LogLevel, ProviderError};

pub struct NoHost;

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

pub const LIBRARY: &str = if cfg!(target_os = "macos") {
    "libclouddeck_extension_toy.dylib"
} else {
    "libclouddeck_extension_toy.so"
};

/// `cargo test` builds only the rlib, so the cdylib is built here into the same target directory.
pub fn built_library() -> PathBuf {
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

pub fn write_manifest(dir: &Path) {
    fs::write(
        dir.join("extension.toml"),
        format!(
            "id = \"toy\"\nname = \"Toy\"\nversion = \"0.1.0\"\nschema_version = 1\napi_version = 4\nlibrary = \"{LIBRARY}\"\n\n[providers.toy]\nname = \"Toy\"\n"
        ),
    )
    .unwrap();
}

/// Sets modes explicitly: the umask of the machine running the tests decides group-write otherwise.
pub fn install(root: &Path) {
    fs::set_permissions(root, fs::Permissions::from_mode(0o755)).unwrap();
    let dir = root.join("toy");
    fs::create_dir_all(&dir).unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
    let library = dir.join(LIBRARY);
    fs::copy(built_library(), &library).unwrap();
    fs::set_permissions(&library, fs::Permissions::from_mode(0o755)).unwrap();
    write_manifest(&dir);
}
