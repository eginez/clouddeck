use semver::Version;

use crate::release::install::{EXTENSION_UNPACK_LIMITS, unpack};
use crate::release::manifest::{check_tag, parse_manifest};
use crate::release::source::FetchedRelease;
use crate::release::target::{current_target, is_newer};
use crate::release::verify::{check_asset, verify_manifest};

use super::index::Index;
use super::install::{MAX_EXTENSION_ASSET, Manager, Outcome};
use super::source::same_repository;
use super::stage::{Staging, read_manifest, remove_shipped_state, swap_in};
use super::store::{installed_record, is_installed, pinned_key, record_install};

pub(super) struct Plan {
    pub(super) owner: String,
    pub(super) repo: String,
    pub(super) repository: String,
    pub(super) expected_id: Option<String>,
    pub(super) offered_key: Option<String>,
    pub(super) accept: Option<String>,
    pub(super) update: bool,
    pub(super) index: Option<Index>,
}

impl Manager {
    pub(super) fn install_release(&self, plan: Plan) -> Result<Outcome, String> {
        let pinned = match &plan.expected_id {
            Some(id) => pinned_key(&self.dirs, id)?,
            None => None,
        };
        let release = self.source.release(&plan.owner, &plan.repo)?;
        let key = choose_key(
            &release,
            pinned.as_deref(),
            plan.offered_key.as_deref(),
            plan.accept.as_deref(),
        )?;
        let manifest = parse_manifest(&release.manifest_bytes)?;
        check_tag(&manifest, &release.tag)?;
        if plan.update {
            let id = plan.expected_id.as_deref().unwrap_or_default();
            let current = Version::parse(&read_manifest(&self.dirs.root.join(id))?.version)
                .map_err(|error| format!("{id} has an invalid version: {error}"))?;
            if !is_newer(&manifest.version, &current) {
                return Ok(Outcome::UpToDate {
                    version: current.to_string(),
                });
            }
        }
        let bytes = self.download_asset(&plan, &release.tag, &manifest)?;
        let staging = Staging::create(
            &self.dirs.root,
            plan.expected_id.as_deref().unwrap_or("release"),
        )?;
        unpack(&bytes, staging.path(), EXTENSION_UNPACK_LIMITS)?;
        let installed = read_manifest(staging.path())?;
        check_expected_id(plan.expected_id.as_deref(), &installed.id)?;
        check_listed_repository(plan.index.as_ref(), &installed.id, &plan.owner, &plan.repo)?;
        remove_shipped_state(staging.path())?;
        installed.library_path(staging.path())?;
        let rotated = self.check_pin(&installed.id, &key, plan.accept.as_deref())?;
        let fresh = rotated || !is_installed(&self.dirs, &installed.id);
        swap_in(&self.dirs.root, &installed.id, staging.path())?;
        record_install(&self.dirs, &installed.id, fresh, |s| {
            s.author_key = Some(key.clone());
            s.repository = Some(plan.repository.clone());
            s.dev = false;
        })?;
        Ok(Outcome::Installed(installed_record(
            &self.dirs, &installed,
        )?))
    }

    fn download_asset(
        &self,
        plan: &Plan,
        tag: &str,
        manifest: &crate::release::manifest::Manifest,
    ) -> Result<Vec<u8>, String> {
        let target = current_target().ok_or(
            "extensions are built for macOS arm64 and Linux x86_64; this platform is unsupported",
        )?;
        let asset = manifest
            .assets
            .get(target.id())
            .ok_or_else(|| format!("release has no asset for {}", target.id()))?;
        if asset.size > MAX_EXTENSION_ASSET {
            return Err(format!(
                "{} is {} bytes; the limit is {MAX_EXTENSION_ASSET}",
                asset.name, asset.size
            ));
        }
        let bytes = self
            .source
            .download(&plan.owner, &plan.repo, tag, &asset.name, asset.size)?;
        check_asset(&bytes, &asset.sha256, asset.size)?;
        Ok(bytes)
    }

    /// Returns true when an accepted key change replaces an existing pin.
    fn check_pin(&self, id: &str, key: &str, accept: Option<&str>) -> Result<bool, String> {
        match pinned_key(&self.dirs, id)? {
            Some(pin) if pin != key && accept != Some(key) => Err(format!(
                "{id} is pinned to author key {pin}, but this release is signed by {key}. \
                 To replace the pin, rerun with --accept-new-key {key}"
            )),
            Some(pin) => Ok(pin != key),
            None => Ok(false),
        }
    }
}

/// The verified release decides the key. A pinned key always governs; a new key is accepted only
/// when the user names the offered key and that key verifies the release.
fn choose_key(
    release: &FetchedRelease,
    pinned: Option<&str>,
    offered: Option<&str>,
    accept: Option<&str>,
) -> Result<String, String> {
    let verifies =
        |key: &str| verify_manifest(&release.manifest_bytes, &release.signature_b64, key).is_ok();
    match (pinned, offered) {
        (Some(pin), _) if verifies(pin) => Ok(pin.to_string()),
        (Some(_), Some(new)) if accept == Some(new) && verifies(new) => Ok(new.to_string()),
        (Some(pin), offered) => Err(format!(
            "the release is not signed by the pinned author key {pin} (offered key: {}). \
             A new key is accepted only with --accept-new-key <offered key>",
            offered.unwrap_or("none")
        )),
        (None, Some(new)) if verifies(new) => Ok(new.to_string()),
        (None, Some(new)) => Err(format!("the release is not signed by the author key {new}")),
        (None, None) => Err("no author key is known for this extension".to_string()),
    }
}

fn check_listed_repository(
    index: Option<&Index>,
    id: &str,
    owner: &str,
    repo: &str,
) -> Result<(), String> {
    match index.and_then(|index| index.find(id)) {
        Some(entry) if !same_repository(&entry.repository, owner, repo) => Err(format!(
            "the index lists {id} at {}, not this repository",
            entry.repository
        )),
        _ => Ok(()),
    }
}

fn check_expected_id(expected: Option<&str>, found: &str) -> Result<(), String> {
    match expected {
        Some(expected) if expected != found => Err(format!(
            "the archive's extension.toml names {found}, but {expected} was requested"
        )),
        _ => Ok(()),
    }
}
