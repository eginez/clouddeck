use std::path::{Path, PathBuf};
use std::process::Command;

use clouddeck_core::{LoadError, load_extension};

/// Fixtures are standalone packages (their own `[workspace]`) so they can set their own profile.
fn build_fixture(dir: &str, stem: &str) -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(dir)
        .join("Cargo.toml");
    let target = Path::new(env!("CARGO_TARGET_TMPDIR")).join(dir);
    let status = Command::new(env!("CARGO"))
        .args(["build", "--manifest-path"])
        .arg(&manifest)
        .arg("--target-dir")
        .arg(&target)
        .status()
        .unwrap();
    assert!(status.success(), "building fixture {dir} failed");
    target.join("debug").join(format!(
        "{}{stem}.{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_EXTENSION
    ))
}

#[test]
fn refuses_a_panic_abort_library() {
    let library = build_fixture("abort-extension", "clouddeck_fixture_abort");
    assert!(matches!(
        load_extension(&library),
        Err(LoadError::BuildInfo { found }) if found == "panic=abort"
    ));
}

#[test]
fn refuses_a_library_exporting_another_api_version() {
    let library = build_fixture("version-extension", "clouddeck_fixture_version");
    assert!(matches!(
        load_extension(&library),
        Err(LoadError::ApiVersion { found: 2 })
    ));
}

#[test]
fn refuses_a_bare_library_name() {
    assert!(matches!(
        load_extension(Path::new("libclouddeck_fixture_abort.so")),
        Err(LoadError::RelativePath { .. })
    ));
}
