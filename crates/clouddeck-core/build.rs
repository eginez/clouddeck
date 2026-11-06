use std::path::PathBuf;
use std::process::Command;

fn main() {
    let commit = git_head().unwrap_or_else(|| "unknown".to_string());
    let channel = if std::env::var("PROFILE").as_deref() == Ok("release") {
        "stable"
    } else {
        "dev"
    };
    println!("cargo:rustc-env=CLOUDDECK_GIT_COMMIT={commit}");
    println!("cargo:rustc-env=CLOUDDECK_CHANNEL={channel}");
    println!("cargo:rerun-if-changed=build.rs");
    for name in ["HEAD", "packed-refs", &branch_ref().unwrap_or_default()] {
        if let Some(path) = git_path(name) {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}

fn branch_ref() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--symbolic-full-name", "HEAD"])
        .current_dir(env_dir())
        .output()
        .ok()?;
    let name = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (output.status.success() && name.starts_with("refs/")).then_some(name)
}

fn git_head() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(env_dir())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let hash = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!hash.is_empty()).then_some(hash)
}

fn git_path(name: &str) -> Option<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--git-path", name])
        .current_dir(env_dir())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
    Some(if path.is_absolute() {
        path
    } else {
        env_dir().join(path)
    })
}

fn env_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default())
}
