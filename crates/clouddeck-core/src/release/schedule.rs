//! Update check scheduling and the state the update window renders.

use std::time::{Duration, SystemTime};

use semver::Version;

use super::manifest::AvailableUpdate;

/// How long to wait between automatic update checks.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// What the update window is currently showing.
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateState {
    /// No update activity has started.
    Idle,
    /// A manifest check is in flight.
    Checking,
    /// The running install is the newest published version.
    UpToDate { version: String },
    /// A newer version is available to install.
    Available(AvailableUpdate),
    /// An update is being downloaded.
    Downloading { version: String, received: u64, total: u64 },
    /// The downloaded update is being installed.
    Installing { version: String },
    /// The new version is installed and waiting for a relaunch.
    ReadyToRelaunch { version: String },
    /// The last update step failed with this message.
    Failed { message: String },
}

/// Whether an automatic check should run now.
pub fn should_auto_check(enabled: bool, last_check: Option<SystemTime>, now: SystemTime) -> bool {
    if !enabled {
        return false;
    }
    match last_check {
        None => true,
        Some(last) => now.duration_since(last).is_ok_and(|elapsed| elapsed >= CHECK_INTERVAL),
    }
}

/// The state to show once a manifest check has finished.
pub fn state_after_check(
    result: Result<Option<AvailableUpdate>, String>,
    current: &Version,
) -> UpdateState {
    match result {
        Ok(Some(update)) => UpdateState::Available(update),
        Ok(None) => UpdateState::UpToDate {
            version: current.to_string(),
        },
        Err(message) => UpdateState::Failed { message },
    }
}

/// The download progress in 0..=1, when the total size is known.
pub fn progress_fraction(state: &UpdateState) -> Option<f32> {
    match state {
        UpdateState::Downloading { received, total, .. } if *total > 0 => {
            Some((*received as f32 / *total as f32).clamp(0.0, 1.0))
        }
        _ => None,
    }
}

/// The one-line status text shown for an update state.
pub fn headline(state: &UpdateState, current: &str) -> String {
    match state {
        UpdateState::Idle => String::new(),
        UpdateState::Checking => "Checking for updates…".to_string(),
        UpdateState::UpToDate { version } => format!("CloudDeck is up to date ({version})"),
        UpdateState::Available(update) => {
            format!("CloudDeck {} is available — you have {current}", update.version)
        }
        UpdateState::Downloading { version, .. } => format!("Downloading CloudDeck {version}…"),
        UpdateState::Installing { version } => format!("Installing CloudDeck {version}…"),
        UpdateState::ReadyToRelaunch { version } => {
            format!("CloudDeck {version} is installed. Relaunch to finish.")
        }
        UpdateState::Failed { message } => format!("Couldn’t update: {message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::release::manifest::Asset;

    fn t() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(2_000_000_000)
    }

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    fn update(version: &str) -> AvailableUpdate {
        AvailableUpdate {
            version: version.to_string(),
            notes: "Notes".to_string(),
            asset: Asset {
                name: "CloudDeck.tar.gz".to_string(),
                sha256: "00".to_string(),
                size: 1,
            },
        }
    }

    #[test]
    fn disabled_never_auto_checks() {
        assert!(!should_auto_check(false, None, t()));
    }

    #[test]
    fn never_checked_auto_checks() {
        assert!(should_auto_check(true, None, t()));
    }

    #[test]
    fn recent_check_waits() {
        let last = t() - Duration::from_secs(23 * 60 * 60);
        assert!(!should_auto_check(true, Some(last), t()));
    }

    #[test]
    fn check_due_after_interval() {
        let last = t() - CHECK_INTERVAL;
        assert!(should_auto_check(true, Some(last), t()));
    }

    #[test]
    fn future_last_check_waits() {
        let last = t() + Duration::from_secs(60);
        assert!(!should_auto_check(true, Some(last), t()));
    }

    #[test]
    fn available_result_becomes_available_state() {
        let u = update("0.3.0");
        assert_eq!(
            state_after_check(Ok(Some(u.clone())), &v("0.2.0")),
            UpdateState::Available(u)
        );
    }

    #[test]
    fn no_update_result_becomes_up_to_date() {
        assert_eq!(
            state_after_check(Ok(None), &v("0.2.0")),
            UpdateState::UpToDate {
                version: "0.2.0".to_string()
            }
        );
    }

    #[test]
    fn error_result_becomes_failed() {
        assert_eq!(
            state_after_check(Err("boom".to_string()), &v("0.2.0")),
            UpdateState::Failed {
                message: "boom".to_string()
            }
        );
    }

    #[test]
    fn progress_is_received_over_total() {
        let state = UpdateState::Downloading {
            version: "0.3.0".to_string(),
            received: 50,
            total: 200,
        };
        assert_eq!(progress_fraction(&state), Some(0.25));
    }

    #[test]
    fn progress_needs_a_known_total() {
        let state = UpdateState::Downloading {
            version: "0.3.0".to_string(),
            received: 50,
            total: 0,
        };
        assert_eq!(progress_fraction(&state), None);
    }

    #[test]
    fn progress_clamps_to_one() {
        let state = UpdateState::Downloading {
            version: "0.3.0".to_string(),
            received: 300,
            total: 200,
        };
        assert_eq!(progress_fraction(&state), Some(1.0));
    }

    #[test]
    fn headline_texts() {
        assert_eq!(headline(&UpdateState::Idle, "0.2.0"), "");
        assert_eq!(
            headline(&UpdateState::Checking, "0.2.0"),
            "Checking for updates…"
        );
        assert_eq!(
            headline(
                &UpdateState::UpToDate {
                    version: "0.2.0".to_string()
                },
                "0.2.0"
            ),
            "CloudDeck is up to date (0.2.0)"
        );
        assert_eq!(
            headline(&UpdateState::Available(update("0.3.0")), "0.2.0"),
            "CloudDeck 0.3.0 is available — you have 0.2.0"
        );
        assert_eq!(
            headline(
                &UpdateState::Downloading {
                    version: "0.3.0".to_string(),
                    received: 0,
                    total: 0
                },
                "0.2.0"
            ),
            "Downloading CloudDeck 0.3.0…"
        );
        assert_eq!(
            headline(
                &UpdateState::Installing {
                    version: "0.3.0".to_string()
                },
                "0.2.0"
            ),
            "Installing CloudDeck 0.3.0…"
        );
        assert_eq!(
            headline(
                &UpdateState::ReadyToRelaunch {
                    version: "0.3.0".to_string()
                },
                "0.2.0"
            ),
            "CloudDeck 0.3.0 is installed. Relaunch to finish."
        );
        assert_eq!(
            headline(
                &UpdateState::Failed {
                    message: "boom".to_string()
                },
                "0.2.0"
            ),
            "Couldn’t update: boom"
        );
    }
}
