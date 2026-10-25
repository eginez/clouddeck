//! In-app update checking and installation.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use clouddeck_core::release::install::install_linux;
#[cfg(target_os = "macos")]
use clouddeck_core::release::install::{bundle_root_from_exe, install_macos};
use clouddeck_core::release::manifest::{
    Asset, AvailableUpdate, check_tag, parse_manifest, update_for,
};
use clouddeck_core::release::schedule::{UpdateState, should_auto_check, state_after_check};
use clouddeck_core::release::source::{FetchedRelease, UpdateSource, github_source_from_env};
use clouddeck_core::release::target::{Target, current_target, current_version};
use clouddeck_core::release::verify::{RELEASE_PUBLIC_KEY, check_asset, verify_manifest};
use clouddeck_core::{Settings, settings_path};
use gpui::{App, AppContext, Context, Entity, EventEmitter};
use semver::Version;

const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
const HOURLY: Duration = Duration::from_secs(60 * 60);
const UNAVAILABLE_MESSAGE: &str = "updates are not available for this platform";

/// Emitted whenever the updater's state changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdaterEvent {
    /// The state returned by Updater::state has changed.
    StateChanged,
}

/// Checks for, downloads, and installs signed CloudDeck releases.
pub struct Updater {
    state: UpdateState,
    last_check: Option<SystemTime>,
    source: Arc<dyn UpdateSource>,
    public_key: String,
    current: Version,
    target: Option<Target>,
    exe: PathBuf,
}

impl EventEmitter<UpdaterEvent> for Updater {}

impl Updater {
    /// Creates an idle updater that reads releases from the given source.
    pub fn new(
        source: Arc<dyn UpdateSource>,
        public_key: String,
        current: Version,
        target: Option<Target>,
        exe: PathBuf,
    ) -> Self {
        Self {
            state: UpdateState::Idle,
            last_check: None,
            source,
            public_key,
            current,
            target,
            exe,
        }
    }

    /// The state the update window should render.
    pub fn state(&self) -> &UpdateState {
        &self.state
    }

    /// Fetches the latest release and moves to Available or UpToDate.
    pub fn check(&mut self, cx: &mut Context<Self>) {
        if !matches!(
            self.state,
            UpdateState::Idle
                | UpdateState::UpToDate { .. }
                | UpdateState::Available(_)
                | UpdateState::Failed { .. }
        ) {
            return;
        }
        let Some(target) = self.target else {
            self.set_state(
                UpdateState::Failed {
                    message: UNAVAILABLE_MESSAGE.to_string(),
                },
                cx,
            );
            return;
        };
        self.last_check = Some(SystemTime::now());
        self.set_state(UpdateState::Checking, cx);

        let source = self.source.clone();
        let public_key = self.public_key.clone();
        let current = self.current.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { fetch_update(source.as_ref(), &public_key, &current, target) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let state = state_after_check(result, &this.current);
                this.set_state(state, cx);
            });
        })
        .detach();
    }

    /// Downloads and installs the offered update, when one is available.
    pub fn install(&mut self, cx: &mut Context<Self>) {
        let UpdateState::Available(update) = self.state.clone() else {
            return;
        };
        let version = update.version.clone();
        let received = Arc::new(AtomicU64::new(0));
        self.set_state(
            UpdateState::Downloading {
                version: version.clone(),
                received: 0,
                total: update.asset.size,
            },
            cx,
        );

        let source = self.source.clone();
        let download_received = received.clone();
        let download = cx.background_executor().spawn(async move {
            fetch_verified(source.as_ref(), &update.asset, &|bytes| {
                download_received.store(bytes, Ordering::Relaxed);
            })
        });
        let exe = self.exe.clone();

        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PROGRESS_INTERVAL).await;
            while let Ok(still_downloading) = this.update(cx, |this, cx| {
                this.set_download_progress(received.load(Ordering::Relaxed), cx)
            }) {
                if !still_downloading {
                    break;
                }
                cx.background_executor().timer(PROGRESS_INTERVAL).await;
            }
        })
        .detach();

        cx.spawn(async move |this, cx| {
            let state = match download.await {
                Err(message) => UpdateState::Failed { message },
                Ok(bytes) => {
                    let _ = this.update(cx, |this, cx| {
                        this.set_state(
                            UpdateState::Installing {
                                version: version.clone(),
                            },
                            cx,
                        );
                    });
                    let installed = cx
                        .background_executor()
                        .spawn(async move { install_into_place(&bytes, &exe) })
                        .await;
                    match installed {
                        Ok(()) => UpdateState::ReadyToRelaunch { version },
                        Err(message) => UpdateState::Failed { message },
                    }
                }
            };
            let _ = this.update(cx, |this, cx| this.set_state(state, cx));
        })
        .detach();
    }

    /// Restarts CloudDeck from the freshly installed build, once one is ready.
    pub fn relaunch(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.state, UpdateState::ReadyToRelaunch { .. }) {
            return;
        }
        match spawn_relaunch(&self.exe) {
            Ok(()) => cx.quit(),
            Err(message) => self.set_state(UpdateState::Failed { message }, cx),
        }
    }

    /// Checks automatically when enabled and a check is due.
    pub fn maybe_auto_check(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if should_auto_check(enabled, self.last_check, SystemTime::now()) {
            self.check(cx);
        }
    }

    fn set_state(&mut self, state: UpdateState, cx: &mut Context<Self>) {
        self.state = state;
        cx.emit(UpdaterEvent::StateChanged);
        cx.notify();
    }

    fn set_download_progress(&mut self, received: u64, cx: &mut Context<Self>) -> bool {
        let UpdateState::Downloading { version, total, .. } = &self.state else {
            return false;
        };
        let state = UpdateState::Downloading {
            version: version.clone(),
            received,
            total: *total,
        };
        self.set_state(state, cx);
        true
    }
}

/// Starts the updater with the release source from the environment and an hourly auto check.
pub fn start(cx: &mut App) -> Entity<Updater> {
    let source: Arc<dyn UpdateSource> = match github_source_from_env() {
        Ok(source) => Arc::new(source),
        Err(message) => Arc::new(UnavailableSource(message)),
    };
    start_with_source(cx, source, settings_path())
}

fn start_with_source(
    cx: &mut App,
    source: Arc<dyn UpdateSource>,
    settings_file: PathBuf,
) -> Entity<Updater> {
    let updater = cx.new(|_| {
        Updater::new(
            source,
            RELEASE_PUBLIC_KEY.to_string(),
            current_version(),
            current_target(),
            std::env::current_exe().unwrap_or_default(),
        )
    });
    crate::update_window::register(cx, &updater);
    updater.update(cx, |updater, cx| {
        updater.maybe_auto_check(auto_check_enabled(&settings_file), cx)
    });

    let hourly = updater.clone();
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(HOURLY).await;
            if hourly
                .update(cx, |updater, cx| {
                    updater.maybe_auto_check(auto_check_enabled(&settings_file), cx)
                })
                .is_err()
            {
                break;
            }
        }
    })
    .detach();
    updater
}

fn auto_check_enabled(settings_file: &Path) -> bool {
    Settings::load(settings_file).check_for_updates
}

struct UnavailableSource(String);

impl UpdateSource for UnavailableSource {
    fn latest(&self) -> Result<FetchedRelease, String> {
        Err(self.0.clone())
    }

    fn download(
        &self,
        _asset_name: &str,
        _max_bytes: u64,
        _progress: &mut dyn FnMut(u64, u64),
    ) -> Result<Vec<u8>, String> {
        Err(self.0.clone())
    }
}

fn fetch_update(
    source: &dyn UpdateSource,
    public_key: &str,
    current: &Version,
    target: Target,
) -> Result<Option<AvailableUpdate>, String> {
    let release = source.latest()?;
    verify_manifest(&release.manifest_bytes, &release.signature_b64, public_key)?;
    let manifest = parse_manifest(&release.manifest_bytes)?;
    check_tag(&manifest, &release.tag)?;
    Ok(update_for(&manifest, current, target))
}

fn fetch_verified(
    source: &dyn UpdateSource,
    asset: &Asset,
    on_bytes: &dyn Fn(u64),
) -> Result<Vec<u8>, String> {
    let bytes = source.download(&asset.name, asset.size, &mut |received, _| {
        on_bytes(received)
    })?;
    check_asset(&bytes, &asset.sha256, asset.size)?;
    Ok(bytes)
}

#[cfg(target_os = "macos")]
fn install_into_place(tar_gz: &[u8], exe: &Path) -> Result<(), String> {
    let bundle = bundle_root_from_exe(exe).ok_or("not running from an app bundle")?;
    install_macos(tar_gz, &bundle)
}

#[cfg(not(target_os = "macos"))]
fn install_into_place(tar_gz: &[u8], exe: &Path) -> Result<(), String> {
    install_linux(tar_gz, exe)
}

#[cfg(target_os = "macos")]
fn spawn_relaunch(exe: &Path) -> Result<(), String> {
    let bundle = bundle_root_from_exe(exe).ok_or("not running from an app bundle")?;
    std::process::Command::new("open")
        .arg("-n")
        .arg(&bundle)
        .spawn()
        .map(drop)
        .map_err(|e| format!("could not relaunch: {e}"))
}

#[cfg(not(target_os = "macos"))]
fn spawn_relaunch(exe: &Path) -> Result<(), String> {
    std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .spawn()
        .map(drop)
        .map_err(|e| format!("could not relaunch: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;
    use std::sync::atomic::AtomicUsize;

    use clouddeck_core::release::manifest::{Manifest, to_canonical_json};
    use clouddeck_core::release::sign::{generate_keypair, sign_bytes};
    use clouddeck_core::release::verify::sha256_hex;
    use gpui::TestAppContext;

    struct FakeSource {
        tag: String,
        manifest_bytes: Vec<u8>,
        signature_b64: String,
        latest_err: Option<String>,
        asset_bytes: Vec<u8>,
        latest_calls: AtomicUsize,
    }

    impl FakeSource {
        fn serving(release: FetchedRelease, asset_bytes: Vec<u8>) -> Arc<Self> {
            Arc::new(Self {
                tag: release.tag,
                manifest_bytes: release.manifest_bytes,
                signature_b64: release.signature_b64,
                latest_err: None,
                asset_bytes,
                latest_calls: AtomicUsize::new(0),
            })
        }

        fn offline() -> Arc<Self> {
            Arc::new(Self {
                tag: String::new(),
                manifest_bytes: Vec::new(),
                signature_b64: String::new(),
                latest_err: Some("offline".to_string()),
                asset_bytes: Vec::new(),
                latest_calls: AtomicUsize::new(0),
            })
        }

        fn latest_calls(&self) -> usize {
            self.latest_calls.load(Ordering::SeqCst)
        }
    }

    impl UpdateSource for FakeSource {
        fn latest(&self) -> Result<FetchedRelease, String> {
            self.latest_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(message) = &self.latest_err {
                return Err(message.clone());
            }
            Ok(FetchedRelease {
                tag: self.tag.clone(),
                manifest_bytes: self.manifest_bytes.clone(),
                signature_b64: self.signature_b64.clone(),
            })
        }

        fn download(
            &self,
            _asset_name: &str,
            _max_bytes: u64,
            progress: &mut dyn FnMut(u64, u64),
        ) -> Result<Vec<u8>, String> {
            let total = self.asset_bytes.len() as u64;
            progress(total, total);
            Ok(self.asset_bytes.clone())
        }
    }

    fn tar_gz_with_binary(content: &[u8]) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::default(),
        ));
        let mut header = tar::Header::new_gnu();
        header.set_path("clouddeck").unwrap();
        header.set_size(content.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder.append(&header, content).unwrap();
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn signed_release(secret: &str, version: &str, asset: &[u8], sha256: String) -> FetchedRelease {
        let manifest = Manifest {
            version: version.to_string(),
            published_at: "2026-10-24T00:00:00Z".to_string(),
            notes: "n".to_string(),
            assets: BTreeMap::from([(
                Target::LinuxX86_64.id().to_string(),
                Asset {
                    name: format!("CloudDeck-{version}-linux-x86_64.tar.gz"),
                    sha256,
                    size: asset.len() as u64,
                },
            )]),
        };
        let manifest_bytes = to_canonical_json(&manifest);
        let signature_b64 = sign_bytes(&manifest_bytes, secret).unwrap();
        FetchedRelease {
            tag: format!("v{version}"),
            manifest_bytes,
            signature_b64,
        }
    }

    struct Scenario {
        secret: String,
        public: String,
        exe: PathBuf,
        _dir: tempfile::TempDir,
    }

    impl Scenario {
        fn new() -> Self {
            let (secret, public) = generate_keypair();
            let dir = tempfile::tempdir().unwrap();
            let exe = dir.path().join("clouddeck");
            fs::write(&exe, b"old").unwrap();
            Self {
                secret,
                public,
                exe,
                _dir: dir,
            }
        }

        fn release_with(&self, version: &str, asset: &[u8]) -> FetchedRelease {
            signed_release(&self.secret, version, asset, sha256_hex(asset))
        }

        fn updater(
            &self,
            cx: &mut TestAppContext,
            source: Arc<FakeSource>,
            target: Option<Target>,
        ) -> Entity<Updater> {
            AppContext::new(cx, |_| {
                Updater::new(
                    source,
                    self.public.clone(),
                    Version::parse("0.2.0").unwrap(),
                    target,
                    self.exe.clone(),
                )
            })
        }
    }

    fn state_of(updater: &Entity<Updater>, cx: &mut TestAppContext) -> UpdateState {
        updater.read_with(cx, |updater, _| updater.state().clone())
    }

    fn exe_content(scenario: &Scenario) -> String {
        fs::read_to_string(&scenario.exe).unwrap()
    }

    #[gpui::test]
    fn check_offers_newer_signed_release(cx: &mut TestAppContext) {
        let scenario = Scenario::new();
        let asset = tar_gz_with_binary(b"new");
        let source = FakeSource::serving(scenario.release_with("0.3.0", &asset), asset);
        let updater = scenario.updater(cx, source, Some(Target::LinuxX86_64));

        updater.update(cx, |updater, cx| updater.check(cx));
        cx.run_until_parked();

        match state_of(&updater, cx) {
            UpdateState::Available(update) => assert_eq!(update.version, "0.3.0"),
            other => panic!("expected Available, got {other:?}"),
        }
    }

    #[gpui::test]
    fn manifest_from_older_release_fails(cx: &mut TestAppContext) {
        let scenario = Scenario::new();
        let asset = tar_gz_with_binary(b"new");
        let release = scenario.release_with("0.2.5", &asset);
        let source = Arc::new(FakeSource {
            tag: "v0.3.0".to_string(),
            manifest_bytes: release.manifest_bytes,
            signature_b64: release.signature_b64,
            latest_err: None,
            asset_bytes: asset,
            latest_calls: AtomicUsize::new(0),
        });
        let updater = scenario.updater(cx, source, Some(Target::LinuxX86_64));

        updater.update(cx, |updater, cx| updater.check(cx));
        cx.run_until_parked();

        assert_eq!(
            state_of(&updater, cx),
            UpdateState::Failed {
                message: "manifest version 0.2.5 does not match release tag v0.3.0".to_string()
            }
        );
    }

    #[gpui::test]
    fn same_version_is_up_to_date(cx: &mut TestAppContext) {
        let scenario = Scenario::new();
        let asset = tar_gz_with_binary(b"new");
        let source = FakeSource::serving(scenario.release_with("0.2.0", &asset), asset);
        let updater = scenario.updater(cx, source, Some(Target::LinuxX86_64));

        updater.update(cx, |updater, cx| updater.check(cx));
        cx.run_until_parked();

        assert_eq!(
            state_of(&updater, cx),
            UpdateState::UpToDate {
                version: "0.2.0".to_string()
            }
        );
    }

    #[gpui::test]
    fn manifest_signed_by_another_key_fails(cx: &mut TestAppContext) {
        let scenario = Scenario::new();
        let (other_secret, _) = generate_keypair();
        let asset = tar_gz_with_binary(b"new");
        let release = signed_release(&other_secret, "0.3.0", &asset, sha256_hex(&asset));
        let source = FakeSource::serving(release, asset);
        let updater = scenario.updater(cx, source, Some(Target::LinuxX86_64));

        updater.update(cx, |updater, cx| updater.check(cx));
        cx.run_until_parked();

        assert_eq!(
            state_of(&updater, cx),
            UpdateState::Failed {
                message: "signature does not match".to_string()
            }
        );
    }

    #[gpui::test]
    fn source_error_becomes_failed(cx: &mut TestAppContext) {
        let scenario = Scenario::new();
        let updater = scenario.updater(cx, FakeSource::offline(), Some(Target::LinuxX86_64));

        updater.update(cx, |updater, cx| updater.check(cx));
        cx.run_until_parked();

        assert_eq!(
            state_of(&updater, cx),
            UpdateState::Failed {
                message: "offline".to_string()
            }
        );
    }

    #[gpui::test]
    fn unsupported_platform_fails_without_fetching(cx: &mut TestAppContext) {
        let scenario = Scenario::new();
        let source = FakeSource::offline();
        let updater = scenario.updater(cx, source.clone(), None);

        updater.update(cx, |updater, cx| updater.check(cx));
        cx.run_until_parked();

        assert_eq!(
            state_of(&updater, cx),
            UpdateState::Failed {
                message: "updates are not available for this platform".to_string()
            }
        );
        assert_eq!(source.latest_calls(), 0);
    }

    #[cfg(target_os = "linux")]
    #[gpui::test]
    fn install_replaces_the_executable(cx: &mut TestAppContext) {
        let scenario = Scenario::new();
        let asset = tar_gz_with_binary(b"new");
        let source = FakeSource::serving(scenario.release_with("0.3.0", &asset), asset);
        let updater = scenario.updater(cx, source, Some(Target::LinuxX86_64));

        updater.update(cx, |updater, cx| updater.check(cx));
        cx.run_until_parked();
        updater.update(cx, |updater, cx| updater.install(cx));
        cx.run_until_parked();

        assert!(matches!(
            state_of(&updater, cx),
            UpdateState::ReadyToRelaunch { .. }
        ));
        assert_eq!(exe_content(&scenario), "new");
    }

    #[gpui::test]
    fn wrong_checksum_leaves_executable_untouched(cx: &mut TestAppContext) {
        let scenario = Scenario::new();
        let asset = tar_gz_with_binary(b"new");
        let release = signed_release(&scenario.secret, "0.3.0", &asset, "0".repeat(64));
        let source = FakeSource::serving(release, asset);
        let updater = scenario.updater(cx, source, Some(Target::LinuxX86_64));

        updater.update(cx, |updater, cx| updater.check(cx));
        cx.run_until_parked();
        updater.update(cx, |updater, cx| updater.install(cx));
        cx.run_until_parked();

        assert_eq!(
            state_of(&updater, cx),
            UpdateState::Failed {
                message: "checksum mismatch".to_string()
            }
        );
        assert_eq!(exe_content(&scenario), "old");
    }

    #[gpui::test]
    fn auto_check_disabled_never_fetches(cx: &mut TestAppContext) {
        let scenario = Scenario::new();
        let source = FakeSource::offline();
        let updater = scenario.updater(cx, source.clone(), Some(Target::LinuxX86_64));

        updater.update(cx, |updater, cx| updater.maybe_auto_check(false, cx));
        cx.run_until_parked();

        assert_eq!(source.latest_calls(), 0);
    }

    #[gpui::test]
    fn start_with_setting_off_makes_no_automatic_check(cx: &mut TestAppContext) {
        let scenario = Scenario::new();
        let settings_file = scenario.exe.with_file_name("settings.json");
        fs::write(&settings_file, br#"{"check_for_updates":false}"#).unwrap();
        let source = FakeSource::offline();

        cx.update(|cx| {
            start_with_source(cx, source.clone(), settings_file);
        });
        cx.run_until_parked();

        assert_eq!(source.latest_calls(), 0);
    }

    #[gpui::test]
    fn auto_check_twice_fetches_once(cx: &mut TestAppContext) {
        let scenario = Scenario::new();
        let source = FakeSource::offline();
        let updater = scenario.updater(cx, source.clone(), Some(Target::LinuxX86_64));

        updater.update(cx, |updater, cx| updater.maybe_auto_check(true, cx));
        updater.update(cx, |updater, cx| updater.maybe_auto_check(true, cx));
        cx.run_until_parked();

        assert_eq!(source.latest_calls(), 1);
    }

    #[gpui::test]
    fn install_from_idle_does_nothing(cx: &mut TestAppContext) {
        let scenario = Scenario::new();
        let asset = tar_gz_with_binary(b"new");
        let source = FakeSource::serving(scenario.release_with("0.3.0", &asset), asset);
        let updater = scenario.updater(cx, source, Some(Target::LinuxX86_64));

        updater.update(cx, |updater, cx| updater.install(cx));
        cx.run_until_parked();

        assert_eq!(state_of(&updater, cx), UpdateState::Idle);
        assert_eq!(exe_content(&scenario), "old");
    }
}
