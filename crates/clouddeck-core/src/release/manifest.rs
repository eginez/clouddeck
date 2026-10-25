//! Signed release manifest model.

use std::collections::BTreeMap;

use semver::Version;
use serde::{Deserialize, Serialize};

use super::target::{is_newer, Target};

/// The manifest.json published with every release.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Manifest {
    /// The release version, without a leading v.
    pub version: String,
    /// When the release was published, as an RFC 3339 timestamp.
    pub published_at: String,
    /// Human-readable release notes.
    pub notes: String,
    /// Downloadable assets keyed by target identifier.
    pub assets: BTreeMap<String, Asset>,
}

/// A single downloadable release asset.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Asset {
    /// The file name of the asset.
    pub name: String,
    /// The lowercase hex SHA-256 digest of the asset.
    pub sha256: String,
    /// The size of the asset in bytes.
    pub size: u64,
}

/// An update offered to the running install.
#[derive(Debug, Clone, PartialEq)]
pub struct AvailableUpdate {
    /// The version being offered.
    pub version: String,
    /// The release notes for the offered version.
    pub notes: String,
    /// The asset to download for this target.
    pub asset: Asset,
}

/// Parses manifest bytes into a Manifest.
pub fn parse_manifest(bytes: &[u8]) -> Result<Manifest, String> {
    serde_json::from_slice(bytes).map_err(|e| format!("invalid manifest: {e}"))
}

/// The exact bytes that are signed: pretty JSON with a trailing newline.
pub fn to_canonical_json(m: &Manifest) -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(m).expect("Manifest serializes to JSON");
    bytes.push(b'\n');
    bytes
}

/// Rejects a manifest whose version does not match the tag of the release that served it.
pub fn check_tag(m: &Manifest, tag: &str) -> Result<(), String> {
    let manifest_version = m.version.strip_prefix('v').unwrap_or(&m.version);
    let tag_version = tag.strip_prefix('v').unwrap_or(tag);
    if manifest_version == tag_version {
        Ok(())
    } else {
        Err(format!(
            "manifest version {} does not match release tag {tag}",
            m.version
        ))
    }
}

/// The update offered by a manifest to a given install, if any.
pub fn update_for(m: &Manifest, current: &Version, target: Target) -> Option<AvailableUpdate> {
    if !is_newer(&m.version, current) {
        return None;
    }
    let asset = m.assets.get(target.id())?.clone();
    Some(AvailableUpdate {
        version: m.version.strip_prefix('v').unwrap_or(&m.version).to_string(),
        notes: m.notes.clone(),
        asset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "a3b1c2d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90";

    const EXAMPLE: &str = r#"{"version":"0.2.0","published_at":"2026-10-24T12:00:00Z","notes":"First release","assets":{"macos-aarch64":{"name":"CloudDeck-0.2.0-macos-aarch64.tar.gz","sha256":"a3b1c2d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90","size":123}}}"#;

    fn asset(name: &str) -> Asset {
        Asset {
            name: name.to_string(),
            sha256: SHA.to_string(),
            size: 123,
        }
    }

    fn manifest(version: &str, targets: &[(&str, Asset)]) -> Manifest {
        let mut assets = BTreeMap::new();
        for (id, a) in targets {
            assets.insert(id.to_string(), a.clone());
        }
        Manifest {
            version: version.to_string(),
            published_at: "2026-10-24T12:00:00Z".to_string(),
            notes: "Notes".to_string(),
            assets,
        }
    }

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn parses_example_manifest() {
        let m = parse_manifest(EXAMPLE.as_bytes()).unwrap();
        assert_eq!(m.version, "0.2.0");
        assert_eq!(m.assets.len(), 1);
        assert_eq!(m.assets["macos-aarch64"].size, 123);
    }

    #[test]
    fn missing_assets_is_an_error() {
        let bytes = br#"{"version":"0.2.0","published_at":"x","notes":"x"}"#;
        let err = parse_manifest(bytes).unwrap_err();
        assert!(err.starts_with("invalid manifest: "), "{err}");
    }

    #[test]
    fn non_json_is_an_error() {
        assert!(parse_manifest(b"not json").is_err());
    }

    #[test]
    fn canonical_json_round_trips() {
        let m = parse_manifest(EXAMPLE.as_bytes()).unwrap();
        let bytes = to_canonical_json(&m);
        assert_eq!(parse_manifest(&bytes).unwrap(), m);
    }

    #[test]
    fn canonical_json_ends_with_newline_and_ignores_insertion_order() {
        let mac = ("macos-aarch64", asset("mac.tar.gz"));
        let linux = ("linux-x86_64", asset("linux.tar.gz"));
        let a = manifest("0.2.0", &[mac.clone(), linux.clone()]);
        let b = manifest("0.2.0", &[linux, mac]);
        let bytes = to_canonical_json(&a);
        assert!(bytes.ends_with(b"\n"));
        assert_eq!(bytes, to_canonical_json(&b));
    }

    #[test]
    fn offers_update_for_newer_version_with_target_asset() {
        let m = manifest("0.2.1", &[("macos-aarch64", asset("mac.tar.gz"))]);
        let update = update_for(&m, &v("0.2.0"), Target::MacosAarch64).unwrap();
        assert_eq!(update.version, "0.2.1");
        assert_eq!(update.asset, asset("mac.tar.gz"));
    }

    #[test]
    fn offered_version_has_no_leading_v() {
        let m = manifest("v0.2.1", &[("macos-aarch64", asset("mac.tar.gz"))]);
        let update = update_for(&m, &v("0.2.0"), Target::MacosAarch64).unwrap();
        assert_eq!(update.version, "0.2.1");
    }

    #[test]
    fn no_update_when_target_asset_is_missing() {
        let m = manifest("0.2.1", &[("macos-aarch64", asset("mac.tar.gz"))]);
        assert!(update_for(&m, &v("0.2.0"), Target::LinuxX86_64).is_none());
    }

    #[test]
    fn no_update_for_same_version() {
        let m = manifest("0.2.0", &[("macos-aarch64", asset("mac.tar.gz"))]);
        assert!(update_for(&m, &v("0.2.0"), Target::MacosAarch64).is_none());
    }

    #[test]
    fn no_update_for_older_version() {
        let m = manifest("0.1.0", &[("macos-aarch64", asset("mac.tar.gz"))]);
        assert!(update_for(&m, &v("0.2.0"), Target::MacosAarch64).is_none());
    }

    #[test]
    fn check_tag_accepts_matching_version_with_or_without_v() {
        let m = manifest("0.2.1", &[]);
        assert!(check_tag(&m, "v0.2.1").is_ok());
        assert!(check_tag(&m, "0.2.1").is_ok());
        let prefixed = manifest("v0.2.1", &[]);
        assert!(check_tag(&prefixed, "v0.2.1").is_ok());
    }

    #[test]
    fn check_tag_rejects_manifest_from_another_release() {
        let older = manifest("0.1.0", &[]);
        let error = check_tag(&older, "v0.2.1").unwrap_err();
        assert_eq!(
            error,
            "manifest version 0.1.0 does not match release tag v0.2.1"
        );
    }

    #[test]
    fn no_update_for_invalid_version() {
        let m = manifest("banana", &[("macos-aarch64", asset("mac.tar.gz"))]);
        assert!(update_for(&m, &v("0.2.0"), Target::MacosAarch64).is_none());
    }
}
