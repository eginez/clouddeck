use std::path::PathBuf;
use std::process::{Command, Output};

fn main() {
    let commit = git_head().unwrap_or_else(|| "unknown".to_string());
    let profile = std::env::var("PROFILE").ok();
    let watched = watched_paths();
    print!(
        "{}",
        directives(&commit, channel(profile.as_deref()), &watched)
    );
}

fn channel(profile: Option<&str>) -> &'static str {
    match profile {
        Some("release") => "stable",
        _ => "dev",
    }
}

fn directives(commit: &str, channel: &str, watched: &[PathBuf]) -> String {
    let watched: String = watched
        .iter()
        .map(|path| format!("cargo:rerun-if-changed={}\n", path.display()))
        .collect();
    format!(
        "cargo:rustc-env=CLOUDDECK_GIT_COMMIT={commit}\n\
         cargo:rustc-env=CLOUDDECK_CHANNEL={channel}\n\
         cargo:rerun-if-changed=build.rs\n{watched}"
    )
}

fn watched_paths() -> Vec<PathBuf> {
    let branch = branch_ref().unwrap_or_default();
    ["HEAD", "packed-refs", branch.as_str()]
        .into_iter()
        .filter_map(git_path)
        .collect()
}

fn branch_ref() -> Option<String> {
    git_stdout(&["rev-parse", "--symbolic-full-name", "HEAD"])
        .filter(|name| name.starts_with("refs/"))
}

fn git_head() -> Option<String> {
    git_stdout(&["rev-parse", "HEAD"]).filter(|hash| !hash.is_empty())
}

fn git_path(name: &str) -> Option<PathBuf> {
    git_stdout(&["rev-parse", "--git-path", name]).map(|path| env_dir().join(path))
}

fn git_stdout(args: &[&str]) -> Option<String> {
    let output = git(args)?;
    output
        .status
        .success()
        .then_some(output.stdout)
        .and_then(|stdout| String::from_utf8(stdout).ok())
        .map(|text| text.trim().to_string())
}

fn git(args: &[&str]) -> Option<Output> {
    Command::new("git")
        .args(args)
        .current_dir(env_dir())
        .output()
        .ok()
}

fn env_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default())
}
