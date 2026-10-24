use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use chrono::DateTime;
use clouddeck_core::release::manifest::{Asset, Manifest, to_canonical_json};
use clouddeck_core::release::sign::{generate_keypair, sign_bytes};
use clouddeck_core::release::target::{Target, asset_name};
use clouddeck_core::release::verify::sha256_hex;
use semver::Version;

const USAGE: &str = "usage: release-manifest keygen --secret-out <path>
       release-manifest build --version <X> --notes-file <file> --asset <target>=<path> [--asset ...] --key <secret> --out <dir> [--published-at <rfc3339>]";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    match run(&args, &now) {
        Ok(out) => println!("{out}"),
        Err((code, message)) => {
            eprintln!("{message}");
            std::process::exit(code);
        }
    }
}

/// Runs the release-manifest command, returning the text to print on success or an exit code and message on failure.
pub fn run(args: &[String], now: &str) -> Result<String, (i32, String)> {
    match args.first().map(String::as_str) {
        Some("keygen") => keygen(&args[1..]),
        Some("build") => build(&args[1..], now),
        _ => Err((2, USAGE.to_string())),
    }
}

fn keygen(args: &[String]) -> Result<String, (i32, String)> {
    let pairs = flag_pairs(args, &["--secret-out"])?;
    let path = required(&pairs, "--secret-out")?;
    let (secret, public) = generate_keypair();
    let mut file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {
            return Err((1, format!("refusing to overwrite {path}")));
        }
        Err(e) => return Err((1, format!("cannot create {path}: {e}"))),
    };
    file.write_all(secret.as_bytes())
        .map_err(|e| (1, format!("cannot write {path}: {e}")))?;
    Ok(format!("public key: {public}"))
}

fn build(args: &[String], now: &str) -> Result<String, (i32, String)> {
    let pairs = flag_pairs(
        args,
        &[
            "--version",
            "--notes-file",
            "--asset",
            "--key",
            "--out",
            "--published-at",
        ],
    )?;
    let version = required(&pairs, "--version")?;
    if Version::parse(&version).is_err() {
        return Err((2, format!("version is not semver: {version}")));
    }
    let notes_file = required(&pairs, "--notes-file")?;
    let key = required(&pairs, "--key")?;
    let out = required(&pairs, "--out")?;
    let published_at = optional(&pairs, "--published-at").unwrap_or_else(|| now.to_string());
    if DateTime::parse_from_rfc3339(&published_at).is_err() {
        return Err((2, format!("--published-at is not RFC 3339: {published_at}")));
    }

    let specs: Vec<&String> = pairs
        .iter()
        .filter(|(name, _)| name == "--asset")
        .map(|(_, spec)| spec)
        .collect();
    if specs.is_empty() {
        return Err((2, "at least one --asset is required".to_string()));
    }
    let mut assets = BTreeMap::new();
    for spec in specs {
        let (id, path) = spec
            .split_once('=')
            .ok_or_else(|| (2, format!("--asset needs <target>=<path>, got {spec}")))?;
        let target = Target::from_id(id).ok_or_else(|| (2, format!("unknown target {id}")))?;
        let expected = asset_name(&version, target);
        let file_name = Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if file_name != expected {
            return Err((2, format!("asset file name must be {expected}")));
        }
        let bytes = fs::read(path).map_err(|e| (1, format!("cannot read {path}: {e}")))?;
        let asset = Asset {
            name: expected,
            sha256: sha256_hex(&bytes),
            size: bytes.len() as u64,
        };
        if assets.insert(id.to_string(), asset).is_some() {
            return Err((2, format!("duplicate asset for {id}")));
        }
    }

    let notes = fs::read_to_string(&notes_file)
        .map_err(|e| (1, format!("cannot read {notes_file}: {e}")))?;
    let secret = fs::read_to_string(&key).map_err(|e| (1, format!("cannot read {key}: {e}")))?;

    let manifest = Manifest {
        version,
        published_at,
        notes,
        assets,
    };
    let bytes = to_canonical_json(&manifest);
    let signature = sign_bytes(&bytes, &secret).map_err(|e| (1, e))?;

    fs::create_dir_all(&out).map_err(|e| (1, format!("cannot create {out}: {e}")))?;
    let manifest_path = Path::new(&out).join("manifest.json");
    let signature_path = Path::new(&out).join("manifest.json.sig");
    fs::write(&manifest_path, &bytes)
        .map_err(|e| (1, format!("cannot write {}: {e}", manifest_path.display())))?;
    fs::write(&signature_path, format!("{signature}\n"))
        .map_err(|e| (1, format!("cannot write {}: {e}", signature_path.display())))?;
    Ok(format!(
        "{}\n{}",
        manifest_path.display(),
        signature_path.display()
    ))
}

fn flag_pairs(args: &[String], allowed: &[&str]) -> Result<Vec<(String, String)>, (i32, String)> {
    let mut pairs = Vec::new();
    let mut iter = args.iter();
    while let Some(name) = iter.next() {
        if !allowed.contains(&name.as_str()) {
            return Err((2, format!("unknown argument {name}")));
        }
        let value = iter
            .next()
            .ok_or_else(|| (2, format!("{name} needs a value")))?;
        pairs.push((name.clone(), value.clone()));
    }
    Ok(pairs)
}

fn optional(pairs: &[(String, String)], name: &str) -> Option<String> {
    pairs
        .iter()
        .find(|(flag, _)| flag == name)
        .map(|(_, value)| value.clone())
}

fn required(pairs: &[(String, String)], name: &str) -> Result<String, (i32, String)> {
    optional(pairs, name).ok_or_else(|| (2, format!("missing {name}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clouddeck_core::release::manifest::parse_manifest;
    use clouddeck_core::release::verify::verify_manifest;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    const NOW: &str = "2026-10-24T12:00:00Z";
    const MAC_ASSET: &str = "CloudDeck-0.2.0-macos-aarch64.tar.gz";
    const LINUX_ASSET: &str = "CloudDeck-0.2.0-linux-x86_64.tar.gz";

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    fn keygen_in(dir: &Path) -> (PathBuf, String) {
        let secret = dir.join("release.key");
        let out = run(
            &args(&["keygen", "--secret-out", secret.to_str().unwrap()]),
            NOW,
        )
        .unwrap();
        let public = out.strip_prefix("public key: ").unwrap().to_string();
        (secret, public)
    }

    fn write_asset(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, b"fake tarball").unwrap();
        path
    }

    fn build_args(dir: &Path, key: &Path, assets: &[String]) -> Vec<String> {
        let notes = dir.join("notes.md");
        fs::write(&notes, "First release").unwrap();
        let mut values = args(&[
            "build",
            "--version",
            "0.2.0",
            "--notes-file",
            notes.to_str().unwrap(),
            "--key",
            key.to_str().unwrap(),
            "--out",
            dir.join("out").to_str().unwrap(),
            "--published-at",
            "2026-10-24T12:00:00Z",
        ]);
        for asset in assets {
            values.push("--asset".into());
            values.push(asset.clone());
        }
        values
    }

    #[test]
    fn keygen_writes_private_secret_and_prints_public_key() {
        let dir = tempfile::tempdir().unwrap();
        let (secret, public) = keygen_in(dir.path());
        let secret_text = fs::read_to_string(&secret).unwrap();
        assert_eq!(secret_text.len(), 44);
        assert_eq!(public.len(), 44);
        let mode = fs::metadata(&secret).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn keygen_refuses_to_overwrite_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let secret = dir.path().join("release.key");
        fs::write(&secret, "existing").unwrap();
        let path = secret.to_str().unwrap();
        let err = run(&args(&["keygen", "--secret-out", path]), NOW).unwrap_err();
        assert_eq!(err, (1, format!("refusing to overwrite {path}")));
        assert_eq!(fs::read_to_string(&secret).unwrap(), "existing");
    }

    #[test]
    fn build_writes_manifest_with_asset_digest() {
        let dir = tempfile::tempdir().unwrap();
        let (key, _) = keygen_in(dir.path());
        let asset = write_asset(dir.path(), MAC_ASSET);
        let spec = format!("macos-aarch64={}", asset.display());
        run(&build_args(dir.path(), &key, &[spec]), NOW).unwrap();

        let bytes = fs::read(dir.path().join("out/manifest.json")).unwrap();
        let manifest = parse_manifest(&bytes).unwrap();
        assert_eq!(manifest.published_at, "2026-10-24T12:00:00Z");
        let entry = &manifest.assets["macos-aarch64"];
        assert_eq!(entry.name, MAC_ASSET);
        assert_eq!(entry.sha256, sha256_hex(b"fake tarball"));
        assert_eq!(entry.size, 12);
    }

    #[test]
    fn build_signature_verifies_against_keygen_public_key() {
        let dir = tempfile::tempdir().unwrap();
        let (key, public) = keygen_in(dir.path());
        let asset = write_asset(dir.path(), MAC_ASSET);
        let spec = format!("macos-aarch64={}", asset.display());
        run(&build_args(dir.path(), &key, &[spec]), NOW).unwrap();

        let out = dir.path().join("out");
        let manifest = fs::read(out.join("manifest.json")).unwrap();
        let signature = fs::read_to_string(out.join("manifest.json.sig")).unwrap();
        assert_eq!(
            verify_manifest(&manifest, signature.trim(), &public),
            Ok(())
        );
    }

    #[test]
    fn build_rejects_misnamed_asset() {
        let dir = tempfile::tempdir().unwrap();
        let (key, _) = keygen_in(dir.path());
        let asset = write_asset(dir.path(), "wrong.tar.gz");
        let spec = format!("macos-aarch64={}", asset.display());
        let err = run(&build_args(dir.path(), &key, &[spec]), NOW).unwrap_err();
        assert_eq!(err, (2, format!("asset file name must be {MAC_ASSET}")));
    }

    #[test]
    fn build_rejects_unknown_target() {
        let dir = tempfile::tempdir().unwrap();
        let (key, _) = keygen_in(dir.path());
        let asset = write_asset(dir.path(), MAC_ASSET);
        let spec = format!("windows-x86_64={}", asset.display());
        let (code, _) = run(&build_args(dir.path(), &key, &[spec]), NOW).unwrap_err();
        assert_eq!(code, 2);
    }

    #[test]
    fn build_rejects_non_semver_version() {
        let dir = tempfile::tempdir().unwrap();
        let (key, _) = keygen_in(dir.path());
        let asset = write_asset(dir.path(), MAC_ASSET);
        let spec = format!("macos-aarch64={}", asset.display());
        let mut values = build_args(dir.path(), &key, &[spec]);
        let index = values.iter().position(|value| value == "0.2.0").unwrap();
        values[index] = "v0.2.0".into();
        let (code, _) = run(&values, NOW).unwrap_err();
        assert_eq!(code, 2);
    }

    #[test]
    fn build_rejects_non_rfc3339_published_at() {
        let dir = tempfile::tempdir().unwrap();
        let (key, _) = keygen_in(dir.path());
        let asset = write_asset(dir.path(), MAC_ASSET);
        let spec = format!("macos-aarch64={}", asset.display());
        let mut values = build_args(dir.path(), &key, &[spec]);
        let index = values
            .iter()
            .position(|value| value == "2026-10-24T12:00:00Z")
            .unwrap();
        values[index] = "yesterday".into();
        let (code, _) = run(&values, NOW).unwrap_err();
        assert_eq!(code, 2);
    }

    #[test]
    fn build_requires_at_least_one_asset() {
        let dir = tempfile::tempdir().unwrap();
        let (key, _) = keygen_in(dir.path());
        let err = run(&build_args(dir.path(), &key, &[]), NOW).unwrap_err();
        assert_eq!(err.0, 2);
    }

    #[test]
    fn build_includes_every_asset() {
        let dir = tempfile::tempdir().unwrap();
        let (key, _) = keygen_in(dir.path());
        let mac = write_asset(dir.path(), MAC_ASSET);
        let linux = write_asset(dir.path(), LINUX_ASSET);
        let specs = [
            format!("macos-aarch64={}", mac.display()),
            format!("linux-x86_64={}", linux.display()),
        ];
        run(&build_args(dir.path(), &key, &specs), NOW).unwrap();

        let bytes = fs::read(dir.path().join("out/manifest.json")).unwrap();
        let manifest = parse_manifest(&bytes).unwrap();
        assert_eq!(manifest.assets.len(), 2);
        assert_eq!(manifest.assets["macos-aarch64"].name, MAC_ASSET);
        assert_eq!(manifest.assets["linux-x86_64"].name, LINUX_ASSET);
    }
}
