use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::Provider;
use crate::extension_host::{MANIFEST_FILE, Manifest, Refusal, StateStore, state_path};

const NAME: &str = "CloudDeck";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const COMMIT: &str = env!("CLOUDDECK_GIT_COMMIT");
const CHANNEL: &str = env!("CLOUDDECK_CHANNEL");
const UNKNOWN: &str = "unknown";
const SHORT_COMMIT_LEN: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginOrigin {
    Bundled,
    Installed,
    Dev,
}

impl PluginOrigin {
    pub fn label(self) -> &'static str {
        match self {
            PluginOrigin::Bundled => "bundled",
            PluginOrigin::Installed => "installed",
            PluginOrigin::Dev => "dev",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginStatus {
    Ok,
    Disabled,
    Refused(String),
}

impl PluginStatus {
    pub fn label(&self) -> String {
        match self {
            PluginStatus::Ok => "ok".to_string(),
            PluginStatus::Disabled => "disabled".to_string(),
            PluginStatus::Refused(reason) => format!("refused: {reason}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginRow {
    pub id: String,
    pub name: String,
    pub version: String,
    pub origin: PluginOrigin,
    pub enabled: bool,
    pub status: PluginStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AboutInfo {
    pub name: String,
    pub version: String,
    pub commit: String,
    pub version_short: String,
    pub version_full: String,
    pub plugins: Vec<PluginRow>,
}

impl AboutInfo {
    fn with_build(version: &str, commit: &str, channel: &str, plugins: Vec<PluginRow>) -> Self {
        let short_commit = commit.get(..SHORT_COMMIT_LEN).unwrap_or(commit);
        Self {
            name: NAME.to_string(),
            version: version.to_string(),
            commit: commit.to_string(),
            version_short: format!("{version}+{channel}.{short_commit}"),
            version_full: format!("{version}+{channel}.{commit}"),
            plugins,
        }
    }

    pub fn copy_text(&self) -> String {
        let mut lines = vec![
            format!("{} {}", self.name, self.version),
            format!("Commit: {}", self.commit),
            format!("Version: {}", self.version_full),
            format!(
                "Platform: {} {}",
                std::env::consts::OS,
                std::env::consts::ARCH
            ),
            "Plugins".to_string(),
        ];
        lines.extend(self.plugins.iter().map(|plugin| {
            format!(
                "{} {} ({}, {})",
                plugin.name,
                plugin.version,
                plugin.origin.label(),
                plugin.status.label()
            )
        }));
        lines.join("\n")
    }
}

/// `refusals` comes from the app's `ProviderRegistry::refusals`, so rows match what the app loads.
pub fn about_info(
    bundled_root: Option<&Path>,
    installed_root: &Path,
    state_root: &Path,
    refusals: &[Refusal],
) -> io::Result<AboutInfo> {
    let mut plugins = vec![demo_row()];
    if let Some(root) = bundled_root {
        plugins.extend(plugin_rows(
            root,
            PluginOrigin::Bundled,
            state_root,
            refusals,
        )?);
    }
    plugins.extend(plugin_rows(
        installed_root,
        PluginOrigin::Installed,
        state_root,
        refusals,
    )?);
    Ok(AboutInfo::with_build(VERSION, COMMIT, CHANNEL, plugins))
}

fn demo_row() -> PluginRow {
    let info = crate::demo_provider().info();
    PluginRow {
        id: info.id,
        name: info.name,
        version: VERSION.to_string(),
        origin: PluginOrigin::Bundled,
        enabled: true,
        status: PluginStatus::Ok,
    }
}

fn plugin_rows(
    root: &Path,
    origin: PluginOrigin,
    state_root: &Path,
    refusals: &[Refusal],
) -> io::Result<Vec<PluginRow>> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && !path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        })
        .collect();
    dirs.sort();
    Ok(dirs
        .iter()
        .map(|dir| plugin_row(dir, origin, state_root, refusals))
        .collect())
}

fn plugin_row(
    dir: &Path,
    origin: PluginOrigin,
    state_root: &Path,
    refusals: &[Refusal],
) -> PluginRow {
    let id = dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let manifest = fs::read_to_string(dir.join(MANIFEST_FILE))
        .ok()
        .and_then(|text| Manifest::parse(&text).ok());
    let (name, version) = match &manifest {
        Some(manifest) => (manifest.name.clone(), manifest.version.clone()),
        None => (id.clone(), UNKNOWN.to_string()),
    };
    let enabled = match origin {
        PluginOrigin::Installed => StateStore::open(&state_path(state_root, &id))
            .is_ok_and(|store| store.read(|state| state.enabled)),
        PluginOrigin::Bundled | PluginOrigin::Dev => true,
    };
    let status = match refusals.iter().find(|refusal| refusal.extension == id) {
        Some(refusal) => PluginStatus::Refused(refusal.message.clone()),
        None if !enabled => PluginStatus::Disabled,
        None => PluginStatus::Ok,
    };
    PluginRow {
        id,
        name,
        version,
        origin,
        enabled,
        status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extension_host::ProviderRegistry;
    use clouddeck_extension_api::API_VERSION;

    const HASH: &str = "0123456789abcdef0123456789abcdef01234567";

    fn manifest_text(id: &str, version: &str, api_version: u32) -> String {
        format!(
            "id = \"{id}\"\nname = \"{id} cloud\"\nversion = \"{version}\"\nschema_version = 1\napi_version = {api_version}\n\n[providers.{id}]\nname = \"{id} cloud\"\n"
        )
    }

    fn install(root: &Path, dir: &str, text: &str) {
        let path = root.join(dir);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join(MANIFEST_FILE), text).unwrap();
    }

    fn set_enabled(state_root: &Path, id: &str, enabled: bool) {
        StateStore::open(&state_path(state_root, id))
            .unwrap()
            .update(|state| state.enabled = enabled)
            .unwrap();
    }

    fn row<'a>(info: &'a AboutInfo, id: &str) -> &'a PluginRow {
        info.plugins.iter().find(|row| row.id == id).unwrap()
    }

    #[test]
    fn plugin_rows_cover_demo_bundled_enabled_disabled_and_refused() {
        let bundled = tempfile::tempdir().unwrap();
        let installed = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        install(
            bundled.path(),
            "aws",
            &manifest_text("aws", "0.2.1", API_VERSION),
        );
        install(
            installed.path(),
            "toy",
            &manifest_text("toy", "0.1.0", API_VERSION),
        );
        set_enabled(state.path(), "toy", true);
        install(
            installed.path(),
            "quiet",
            &manifest_text("quiet", "0.3.0", API_VERSION),
        );
        install(
            installed.path(),
            "old",
            &manifest_text("old", "0.1.0", API_VERSION + 1),
        );
        set_enabled(state.path(), "old", true);

        let mut registry = ProviderRegistry::default().with_external(true);
        registry
            .discover_bundled(bundled.path(), state.path())
            .unwrap();
        registry.discover(installed.path(), state.path()).unwrap();
        let info = about_info(
            Some(bundled.path()),
            installed.path(),
            state.path(),
            &registry.refusals(),
        )
        .unwrap();

        assert_eq!(info.plugins.len(), 5);
        assert_eq!(row(&info, "demo").origin, PluginOrigin::Bundled);
        assert_eq!(row(&info, "demo").version, VERSION);
        assert_eq!(
            row(&info, "aws"),
            &PluginRow {
                id: "aws".into(),
                name: "aws cloud".into(),
                version: "0.2.1".into(),
                origin: PluginOrigin::Bundled,
                enabled: true,
                status: PluginStatus::Ok,
            }
        );
        assert_eq!(row(&info, "toy").status, PluginStatus::Ok);
        assert_eq!(row(&info, "toy").origin, PluginOrigin::Installed);
        assert!(row(&info, "toy").enabled);
        assert_eq!(row(&info, "quiet").status, PluginStatus::Disabled);
        assert!(!row(&info, "quiet").enabled);
        assert!(matches!(
            &row(&info, "old").status,
            PluginStatus::Refused(reason) if reason.contains("Rebuild old for CloudDeck")
        ));
    }

    #[test]
    fn copy_text_lists_build_and_one_line_per_plugin() {
        let rows = vec![
            PluginRow {
                id: "demo".into(),
                name: "Demo".into(),
                version: "0.2.1".into(),
                origin: PluginOrigin::Bundled,
                enabled: true,
                status: PluginStatus::Ok,
            },
            PluginRow {
                id: "quiet".into(),
                name: "Quiet".into(),
                version: "0.3.0".into(),
                origin: PluginOrigin::Installed,
                enabled: false,
                status: PluginStatus::Disabled,
            },
            PluginRow {
                id: "old".into(),
                name: "Old".into(),
                version: "0.1.0".into(),
                origin: PluginOrigin::Dev,
                enabled: true,
                status: PluginStatus::Refused("built for API 5".into()),
            },
        ];
        let info = AboutInfo::with_build("0.2.1", HASH, "stable", rows);

        assert_eq!(
            info.copy_text(),
            format!(
                "CloudDeck 0.2.1\nCommit: {HASH}\nVersion: 0.2.1+stable.{HASH}\nPlatform: {} {}\nPlugins\nDemo 0.2.1 (bundled, ok)\nQuiet 0.3.0 (installed, disabled)\nOld 0.1.0 (dev, refused: built for API 5)",
                std::env::consts::OS,
                std::env::consts::ARCH,
            )
        );
    }

    #[test]
    fn version_full_joins_version_channel_and_commit() {
        assert_eq!(
            AboutInfo::with_build("0.2.1", HASH, "stable", vec![]).version_full,
            format!("0.2.1+stable.{HASH}")
        );
        assert_eq!(
            AboutInfo::with_build("0.2.1", UNKNOWN, "dev", vec![]).version_full,
            "0.2.1+dev.unknown"
        );
    }

    #[test]
    fn version_short_keeps_eight_commit_chars() {
        assert_eq!(
            AboutInfo::with_build("0.2.1", HASH, "dev", vec![]).version_short,
            "0.2.1+dev.01234567"
        );
        assert_eq!(
            AboutInfo::with_build("0.2.1", UNKNOWN, "dev", vec![]).version_short,
            "0.2.1+dev.unknown"
        );
    }

    #[test]
    fn commit_is_a_hash_or_unknown() {
        assert!(
            COMMIT == UNKNOWN
                || (!COMMIT.is_empty() && COMMIT.chars().all(|c| c.is_ascii_hexdigit()))
        );
    }
}
