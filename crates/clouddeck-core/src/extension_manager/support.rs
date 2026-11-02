use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use clouddeck_extension_api::API_VERSION;

use crate::extension_host::{StateStore, state_path};
use crate::extension_manager::index::{INDEX_SCHEMA, IndexError};
use crate::extension_manager::install::Manager;
use crate::extension_manager::source::ExtensionSource;
use crate::release::manifest::{Asset, Manifest as ReleaseManifest, to_canonical_json};
use crate::release::sign::{generate_keypair, sign_bytes};
use crate::release::source::FetchedRelease;
use crate::release::target::current_target as target_now;
use crate::release::verify::sha256_hex;

#[derive(Default)]
pub(super) struct Hub {
    /// `None` stands for an index that cannot be fetched (a 404 in production).
    pub(super) index: Option<(Vec<u8>, String)>,
    pub(super) tag: String,
    pub(super) manifest: Vec<u8>,
    pub(super) signature: String,
    pub(super) assets: BTreeMap<String, Vec<u8>>,
}

pub(super) struct Fake(Arc<Mutex<Hub>>);

impl Fake {
    fn hub(&self) -> MutexGuard<'_, Hub> {
        self.0.lock().unwrap()
    }
}

impl ExtensionSource for Fake {
    fn index(&self) -> Result<(Vec<u8>, String), IndexError> {
        self.hub()
            .index
            .clone()
            .ok_or_else(|| IndexError::Unavailable("404 Not Found".to_string()))
    }

    fn release(&self, _owner: &str, _repo: &str) -> Result<FetchedRelease, String> {
        let hub = self.hub();
        Ok(FetchedRelease {
            tag: hub.tag.clone(),
            manifest_bytes: hub.manifest.clone(),
            signature_b64: hub.signature.clone(),
        })
    }

    fn download(
        &self,
        _owner: &str,
        _repo: &str,
        _tag: &str,
        asset: &str,
        max_bytes: u64,
    ) -> Result<Vec<u8>, String> {
        let bytes = self
            .hub()
            .assets
            .get(asset)
            .cloned()
            .ok_or_else(|| format!("no asset {asset}"))?;
        if bytes.len() as u64 > max_bytes {
            return Err("too large".to_string());
        }
        Ok(bytes)
    }
}

pub(super) const REPO: &str = "https://github.com/eginez/clouddeck-toy";

pub(super) struct Fixture {
    pub(super) _dir: tempfile::TempDir,
    pub(super) root: PathBuf,
    pub(super) state_root: PathBuf,
    pub(super) hub: Arc<Mutex<Hub>>,
    pub(super) app_key: (String, String),
    pub(super) author: (String, String),
}

pub(super) fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    Fixture {
        root: dir.path().join("extensions"),
        state_root: dir.path().join("state"),
        _dir: dir,
        hub: Arc::new(Mutex::new(Hub::default())),
        app_key: generate_keypair(),
        author: generate_keypair(),
    }
}

pub(super) fn library_name(id: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("lib{id}.dylib")
    } else {
        format!("lib{id}.so")
    }
}

pub(super) fn extension_toml(id: &str, api: u32, version: &str) -> String {
    format!(
        "id = \"{id}\"\nname = \"Toy\"\nversion = \"{version}\"\nschema_version = 1\napi_version = {api}\n\n[providers.{id}]\nname = \"Toy cloud\"\n"
    )
}

pub(super) fn archive(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for (name, data) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o755);
        header.set_path(name).unwrap();
        header.set_cksum();
        builder.append(&header, *data).unwrap();
    }
    gzip(builder.into_inner().unwrap())
}

pub(super) fn traversal_archive(first: &[u8]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_size(first.len() as u64);
    header.set_mode(0o644);
    header.set_path("extension.toml").unwrap();
    header.set_cksum();
    builder.append(&header, first).unwrap();
    let mut escape = tar::Header::new_gnu();
    escape.set_size(4);
    escape.set_mode(0o644);
    escape.set_path("placeholder").unwrap();
    let name = b"../escape";
    escape.as_old_mut().name[..name.len()].copy_from_slice(name);
    escape.set_cksum();
    builder.append(&escape, &b"evil"[..]).unwrap();
    gzip(builder.into_inner().unwrap())
}

pub(super) fn gzip(bytes: Vec<u8>) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&bytes).unwrap();
    encoder.finish().unwrap()
}

pub(super) fn standard_archive(id: &str, api: u32, version: &str) -> Vec<u8> {
    archive(&[
        (
            "extension.toml",
            extension_toml(id, api, version).as_bytes(),
        ),
        (&library_name(id), b"fake library"),
    ])
}

impl Fixture {
    pub(super) fn manager(&self) -> Manager {
        Manager::new(
            self.root.clone(),
            self.state_root.clone(),
            Box::new(Fake(Arc::clone(&self.hub))),
            self.app_key.1.clone(),
        )
    }

    pub(super) fn publish_index(&self, author_key: &str) {
        let json = serde_json::json!({
            "schema": INDEX_SCHEMA,
            "extensions": [{"id": "toy", "repository": REPO, "author_key": author_key}],
        })
        .to_string();
        let sig = sign_bytes(json.as_bytes(), &self.app_key.0).unwrap();
        self.hub.lock().unwrap().index = Some((json.into_bytes(), sig));
    }

    pub(super) fn publish_empty_index(&self) {
        let json = serde_json::json!({"schema": INDEX_SCHEMA, "extensions": []}).to_string();
        let sig = sign_bytes(json.as_bytes(), &self.app_key.0).unwrap();
        self.hub.lock().unwrap().index = Some((json.into_bytes(), sig));
    }

    pub(super) fn publish_forged_index(&self) {
        let json = serde_json::json!({"schema": INDEX_SCHEMA, "extensions": []}).to_string();
        let sig = sign_bytes(json.as_bytes(), &generate_keypair().0).unwrap();
        self.hub.lock().unwrap().index = Some((json.into_bytes(), sig));
    }

    pub(super) fn hide_index(&self) {
        self.hub.lock().unwrap().index = None;
    }

    pub(super) fn publish_release(&self, secret: &str, version: &str, bundle: &[u8]) {
        let target = target_now().unwrap().id();
        let name = format!("toy-{version}-{target}.tar.gz");
        let manifest = ReleaseManifest {
            version: version.to_string(),
            published_at: "2026-11-02T00:00:00Z".to_string(),
            notes: String::new(),
            assets: BTreeMap::from([(
                target.to_string(),
                Asset {
                    name: name.clone(),
                    sha256: sha256_hex(bundle),
                    size: bundle.len() as u64,
                },
            )]),
        };
        let bytes = to_canonical_json(&manifest);
        let signature = sign_bytes(&bytes, secret).unwrap();
        let mut hub = self.hub.lock().unwrap();
        hub.tag = format!("v{version}");
        hub.manifest = bytes;
        hub.signature = signature;
        hub.assets.insert(name, bundle.to_vec());
    }

    pub(super) fn installed_pin(&self) -> Option<String> {
        StateStore::open(&state_path(&self.state_root, "toy"))
            .unwrap()
            .read(|s| s.author_key.clone())
    }

    pub(super) fn state_enabled(&self) -> bool {
        StateStore::open(&state_path(&self.state_root, "toy"))
            .unwrap()
            .read(|s| s.enabled)
    }

    pub(super) fn install_v1(&self) {
        self.publish_index(&self.author.1);
        self.publish_release(
            &self.author.0,
            "0.1.0",
            &standard_archive("toy", API_VERSION, "0.1.0"),
        );
        self.manager().install_from_index("toy", None).unwrap();
    }

    pub(super) fn staging_leftovers(&self) -> usize {
        fs::read_dir(&self.root)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter(|e| e.file_name().to_string_lossy().starts_with(".tmp-"))
                    .count()
            })
            .unwrap_or(0)
    }
}

pub(super) fn read(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap()
}
