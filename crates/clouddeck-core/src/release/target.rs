//! Release target platform identifiers.

use semver::Version;

/// A platform that CloudDeck ships release builds for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// macOS on Apple silicon.
    MacosAarch64,
    /// Linux on x86_64.
    LinuxX86_64,
}

impl Target {
    /// The identifier used in asset names.
    pub fn id(&self) -> &'static str {
        match self {
            Target::MacosAarch64 => "macos-aarch64",
            Target::LinuxX86_64 => "linux-x86_64",
        }
    }

    /// Parses a target identifier, returning None for unknown targets.
    pub fn from_id(id: &str) -> Option<Target> {
        match id {
            "macos-aarch64" => Some(Target::MacosAarch64),
            "linux-x86_64" => Some(Target::LinuxX86_64),
            _ => None,
        }
    }
}

/// The target this binary was compiled for, if it is a shipped target.
pub fn current_target() -> Option<Target> {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some(Target::MacosAarch64)
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some(Target::LinuxX86_64)
    } else {
        None
    }
}

/// The tarball asset name for a version and target.
pub fn asset_name(version: &str, target: Target) -> String {
    format!("CloudDeck-{version}-{}.tar.gz", target.id())
}

/// The macOS disk image asset name for a version.
pub fn dmg_name(version: &str) -> String {
    format!("CloudDeck-{version}-macos-aarch64.dmg")
}

/// The running version of CloudDeck; release.sh keeps the core and app crate versions equal.
pub fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("CARGO_PKG_VERSION is valid semver")
}

/// Whether a candidate version string (optionally prefixed with v) is strictly newer than current.
pub fn is_newer(candidate: &str, current: &Version) -> bool {
    let trimmed = candidate.strip_prefix('v').unwrap_or(candidate);
    match Version::parse(trimmed) {
        Ok(version) => version > *current,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn target_ids_round_trip() {
        assert_eq!(Target::MacosAarch64.id(), "macos-aarch64");
        assert_eq!(Target::LinuxX86_64.id(), "linux-x86_64");
        assert_eq!(Target::from_id("linux-x86_64"), Some(Target::LinuxX86_64));
        assert_eq!(Target::from_id("macos-aarch64"), Some(Target::MacosAarch64));
        assert_eq!(Target::from_id("windows-x86_64"), None);
    }

    #[test]
    fn asset_names() {
        assert_eq!(
            asset_name("0.2.0", Target::MacosAarch64),
            "CloudDeck-0.2.0-macos-aarch64.tar.gz"
        );
        assert_eq!(dmg_name("0.2.0"), "CloudDeck-0.2.0-macos-aarch64.dmg");
    }

    #[test]
    fn is_newer_compares_strictly() {
        let current = v("0.2.0");
        assert!(is_newer("0.2.1", &current));
        assert!(is_newer("v0.3.0", &current));
        assert!(is_newer("0.3.0-beta.1", &current));
        assert!(!is_newer("0.2.0", &current));
        assert!(!is_newer("0.1.9", &current));
        assert!(!is_newer("banana", &current));
    }

    #[test]
    fn current_version_matches_package() {
        assert_eq!(current_version().to_string(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    fn current_target_on_linux_x86_64() {
        assert_eq!(current_target(), Some(Target::LinuxX86_64));
    }
}
