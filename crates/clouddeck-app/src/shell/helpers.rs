use super::*;

pub(super) fn restored_regions(saved: &[String], available: &[String]) -> Vec<String> {
    let kept: Vec<String> = available
        .iter()
        .filter(|region| saved.contains(region))
        .cloned()
        .collect();
    if kept.is_empty() {
        available.to_vec()
    } else {
        kept
    }
}

pub(super) fn accepts_provider(id: &str, known: bool) -> Option<()> {
    if id == ALL_PROVIDERS || known {
        Some(())
    } else {
        None
    }
}

pub(super) fn keeps_switch(switched: bool, needs_sign_in: bool) -> Option<()> {
    if switched || needs_sign_in {
        Some(())
    } else {
        None
    }
}

pub(super) fn record_save_failure(
    message: &mut Option<String>,
    saved: Option<std::io::Result<()>>,
) {
    if let Some(Err(error)) = saved {
        *message = Some(format!("Could not remember this provider: {error}"));
    }
}

pub(super) fn changes_aws_profiles(extension: &str, key: Option<&str>) -> bool {
    extension == "aws" && key.is_none_or(|key| matches!(key, "config_dir" | "config_file"))
}

pub(super) fn reload_target(
    shows_all: bool,
    needs_sign_in: bool,
    current: WorkspaceContext,
    provider: &str,
    first_profile: Option<String>,
) -> Option<WorkspaceContext> {
    if shows_all {
        return Some(current);
    }
    match first_profile {
        Some(profile) if !needs_sign_in => Some(WorkspaceContext {
            provider: provider.to_string(),
            profile,
        }),
        _ => None,
    }
}

#[cfg(test)]
thread_local! {
    pub(super) static TEST_CONTEXT_PATH: PathBuf = std::env::temp_dir().join(format!(
        "clouddeck-test-context-{}-{:?}.json",
        std::process::id(),
        std::thread::current().id()
    ));
}

pub(crate) fn saved_context_path() -> PathBuf {
    #[cfg(test)]
    return TEST_CONTEXT_PATH.with(PathBuf::clone);
    #[cfg(not(test))]
    clouddeck_core::context_path()
}

pub(super) fn should_remember_context(demo: bool, usable: bool) -> bool {
    !demo && usable
}

pub(super) fn restore_ui_state(
    path: PathBuf,
    loaded: std::io::Result<UiState>,
) -> (UiState, Option<PathBuf>, Option<String>) {
    loaded.map_or_else(
        |error| {
            eprintln!("Could not read UI state; leaving it unchanged: {error}");
            (
                UiState::default(),
                None,
                Some(format!(
                    "Could not read UI state, so it will not be saved this session: {error}"
                )),
            )
        },
        |state| (state, Some(path), None),
    )
}

/// Tests never pass `--demo`, so `cargo test` runs always see real mode here.
#[cfg(test)]
pub(super) fn is_real_mode() -> bool {
    !std::env::args_os().any(|arg| arg == "--demo")
}
