use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::Provider;
use crate::about::PluginOrigin;
use crate::extension_host::{
    Capability, MANIFEST_FILE, Manifest, Refusal, State, StateStore, state_path,
};
use crate::extension_manager::index::IndexEntry;

use super::install::{Manager, Outcome};

pub const NOT_AVAILABLE: &str = "Not available in this build";
pub const AVAILABLE_EMPTY: &str =
    "No plugin index is configured yet. Plugins you install from a folder appear under Installed.";
pub const NO_MATCHES: &str = "No plugins match your search.";
const ALL_PLATFORMS: &str = "All platforms";
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Installed,
    Available,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Enable,
    Disable,
    Update,
    Install,
    Remove,
}

impl Action {
    fn verb(self) -> &'static str {
        match self {
            Action::Enable => "Enabled",
            Action::Disable => "Disabled",
            Action::Update => "Updated",
            Action::Install => "Installed",
            Action::Remove => "Removed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    pub action: Action,
    pub blocked: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Badge {
    Bundled,
    Installed,
    Dev,
    Disabled,
    Refused(String),
    UpdateAvailable(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginRowVm {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub badges: Vec<Badge>,
    pub primary: Option<Slot>,
    pub secondary: Option<Slot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityView {
    pub capability: Capability,
    pub granted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginDetail {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub repository: Option<String>,
    pub platforms: String,
    pub capabilities: Vec<CapabilityView>,
    pub status_lines: Vec<String>,
    pub install_path: Option<PathBuf>,
}

/// Everything the pane knows about one installed, bundled or dev extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub origin: PluginOrigin,
    pub authors: Vec<String>,
    pub description: String,
    pub repository: Option<String>,
    pub platforms: Vec<String>,
    pub capabilities: Vec<Capability>,
    pub granted: Vec<Capability>,
    pub enabled: bool,
    pub crash_disabled: bool,
    pub refusal: Option<String>,
    pub install_dir: Option<PathBuf>,
}

impl PluginEntry {
    fn bare(id: &str, name: &str, origin: PluginOrigin) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            version: VERSION.to_string(),
            origin,
            authors: Vec::new(),
            description: String::new(),
            repository: None,
            platforms: Vec::new(),
            capabilities: Vec::new(),
            granted: Vec::new(),
            enabled: true,
            crash_disabled: false,
            refusal: None,
            install_dir: None,
        }
    }
}

/// Reads bundled, installed and demo entries. Bundled entries are always on.
pub fn load_entries(
    bundled_root: Option<&Path>,
    installed_root: &Path,
    state_root: &Path,
    refusals: &[Refusal],
) -> io::Result<Vec<PluginEntry>> {
    let demo = crate::demo_provider().info();
    let mut entries = vec![PluginEntry::bare(
        &demo.id,
        &demo.name,
        PluginOrigin::Bundled,
    )];
    if let Some(root) = bundled_root {
        entries.extend(read_folder(root, state_root, refusals, true)?);
    }
    entries.extend(read_folder(installed_root, state_root, refusals, false)?);
    Ok(entries)
}

fn read_folder(
    root: &Path,
    state_root: &Path,
    refusals: &[Refusal],
    bundled: bool,
) -> io::Result<Vec<PluginEntry>> {
    let mut dirs: Vec<PathBuf> = match fs::read_dir(root) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_dir()
                    && !path
                        .file_name()
                        .is_some_and(|name| name.to_string_lossy().starts_with('.'))
            })
            .collect(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    dirs.sort();
    Ok(dirs
        .iter()
        .filter_map(|dir| read_entry(dir, state_root, refusals, bundled))
        .collect())
}

fn read_entry(
    dir: &Path,
    state_root: &Path,
    refusals: &[Refusal],
    bundled: bool,
) -> Option<PluginEntry> {
    let text = fs::read_to_string(dir.join(MANIFEST_FILE)).ok()?;
    let manifest = Manifest::parse(&text).ok()?;
    if !dir.ends_with(&manifest.id) {
        return None;
    }
    let state = StateStore::open(&state_path(state_root, &manifest.id))
        .ok()
        .map(|store| store.read(State::clone))
        .unwrap_or_default();
    let origin = match (bundled, state.dev) {
        (true, _) => PluginOrigin::Bundled,
        (false, true) => PluginOrigin::Dev,
        (false, false) => PluginOrigin::Installed,
    };
    Some(PluginEntry {
        refusal: refusals
            .iter()
            .find(|refusal| refusal.extension == manifest.id)
            .map(|refusal| refusal.message.clone()),
        id: manifest.id,
        name: manifest.name,
        version: manifest.version,
        origin,
        authors: manifest.authors,
        description: manifest.description,
        repository: manifest.repository,
        platforms: manifest.platforms,
        capabilities: manifest.capabilities,
        granted: state.granted.clone(),
        enabled: bundled || state.enabled,
        crash_disabled: !bundled && state.crash_disabled(),
        install_dir: Some(dir.to_path_buf()),
    })
}

/// The extension manager's operations as the pane needs them. `Manager` implements it.
pub trait PluginActions {
    fn enable(&self, id: &str) -> Result<(), String>;
    fn disable(&self, id: &str) -> Result<(), String>;
    fn remove(&self, id: &str) -> Result<(), String>;
    fn install(&self, id: &str) -> Result<Outcome, String>;
    fn update(&self, id: &str) -> Result<Outcome, String>;
    fn install_dev(&self, path: &Path) -> Result<Outcome, String>;
}

impl PluginActions for Manager {
    fn enable(&self, id: &str) -> Result<(), String> {
        Manager::enable(self, id)
    }

    fn disable(&self, id: &str) -> Result<(), String> {
        Manager::disable(self, id)
    }

    fn remove(&self, id: &str) -> Result<(), String> {
        Manager::remove(self, id)
    }

    fn install(&self, id: &str) -> Result<Outcome, String> {
        Manager::install_from_index(self, id, None)
    }

    fn update(&self, id: &str) -> Result<Outcome, String> {
        Manager::update(self, id, None)
    }

    fn install_dev(&self, path: &Path) -> Result<Outcome, String> {
        Manager::install_dev(self, path)
    }
}

#[derive(Debug, Clone)]
pub struct PluginsViewModel {
    pub tab: Tab,
    pub query: String,
    pub selected: Option<String>,
    pub rows: Vec<PluginRowVm>,
    pub detail: Option<PluginDetail>,
    entries: Vec<PluginEntry>,
    available: Vec<IndexEntry>,
    updates: BTreeMap<String, String>,
    external_allowed: bool,
    restart_required: BTreeSet<String>,
}

impl PluginsViewModel {
    pub fn new(
        entries: Vec<PluginEntry>,
        available: Vec<IndexEntry>,
        updates: BTreeMap<String, String>,
        external_allowed: bool,
    ) -> Self {
        let mut model = Self {
            tab: Tab::default(),
            query: String::new(),
            selected: None,
            rows: Vec::new(),
            detail: None,
            entries,
            available,
            updates,
            external_allowed,
            restart_required: BTreeSet::new(),
        };
        model.rows = model.build_rows();
        model
    }

    pub fn set_tab(&mut self, tab: Tab) {
        self.tab = tab;
        self.rebuild_rows();
    }

    pub fn set_query(&mut self, query: &str) {
        self.query = query.to_string();
        self.rebuild_rows();
    }

    fn rebuild_rows(&mut self) {
        self.rows = self.build_rows();
        let still_listed = self
            .selected
            .as_ref()
            .is_some_and(|id| self.rows.iter().any(|row| &row.id == id));
        if !still_listed {
            self.selected = None;
            self.detail = None;
        }
    }

    /// Replaces the installed entries after an action, so rows reflect what is on disk now.
    pub fn set_entries(&mut self, entries: Vec<PluginEntry>) {
        self.entries = entries;
        self.rebuild_rows();
        self.detail = self.selected.clone().and_then(|id| self.detail_for(&id));
    }

    pub fn select(&mut self, id: Option<&str>) {
        self.selected = id.map(str::to_string);
        self.detail = id.and_then(|id| self.detail_for(id));
    }

    /// Ids whose changes take effect only after CloudDeck restarts.
    pub fn restart_required(&self) -> Vec<String> {
        self.restart_required.iter().cloned().collect()
    }

    /// Display names of the plugins in `restart_required`, falling back to the id.
    pub fn restart_required_names(&self) -> Vec<String> {
        self.restart_required
            .iter()
            .map(|id| {
                self.entries
                    .iter()
                    .find(|entry| &entry.id == id)
                    .map_or_else(|| id.clone(), |entry| entry.name.clone())
            })
            .collect()
    }

    pub fn installed_count(&self) -> usize {
        self.entries.len()
    }

    pub fn empty_state(&self) -> Option<&'static str> {
        if !self.rows.is_empty() {
            return None;
        }
        if !self.query.is_empty() {
            return Some(NO_MATCHES);
        }
        match self.tab {
            Tab::Available if self.available.is_empty() => Some(AVAILABLE_EMPTY),
            _ => None,
        }
    }

    /// Runs one row action. Blocked actions return their reason without calling the manager.
    pub fn act(
        &mut self,
        actions: &dyn PluginActions,
        id: &str,
        action: Action,
    ) -> Result<String, String> {
        let row = self
            .row_for(id)
            .ok_or_else(|| format!("{id} is not listed"))?;
        let slot = [row.primary.as_ref(), row.secondary.as_ref()]
            .into_iter()
            .flatten()
            .find(|slot| slot.action == action)
            .ok_or_else(|| format!("{action:?} is not available for {id}"))?;
        if let Some(reason) = slot.blocked {
            return Err(reason.to_string());
        }
        match action {
            Action::Enable => actions.enable(id)?,
            Action::Disable => actions.disable(id)?,
            Action::Remove => actions.remove(id)?,
            Action::Update | Action::Install => {
                let outcome = if action == Action::Update {
                    actions.update(id)?
                } else {
                    actions.install(id)?
                };
                if let Outcome::UpToDate { .. } = outcome {
                    return Ok(format!("{} is up to date.", row.name));
                }
            }
        }
        self.restart_required.insert(id.to_string());
        Ok(format!(
            "{} {}. Restart CloudDeck to apply.",
            action.verb(),
            row.name
        ))
    }

    pub fn install_dev(
        &mut self,
        actions: &dyn PluginActions,
        path: &Path,
    ) -> Result<String, String> {
        let id = match actions.install_dev(path)? {
            Outcome::Installed(installed) => installed.id,
            Outcome::UpToDate { .. } => {
                return Err(format!("{} is already installed", path.display()));
            }
        };
        self.restart_required.insert(id.clone());
        Ok(format!("Installed {id}. Restart CloudDeck to apply."))
    }

    fn build_rows(&self) -> Vec<PluginRowVm> {
        let mut rows: Vec<PluginRowVm> = match self.tab {
            Tab::Installed => self
                .entries
                .iter()
                .map(|entry| self.installed_row(entry))
                .collect(),
            Tab::Available => self
                .available
                .iter()
                .filter(|entry| !self.entries.iter().any(|plugin| plugin.id == entry.id))
                .map(|entry| available_row(entry, self.external_allowed))
                .collect(),
        };
        rows.retain(|row| self.matches(row));
        rows.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
        rows
    }

    fn matches(&self, row: &PluginRowVm) -> bool {
        let query = self.query.to_lowercase();
        query.is_empty()
            || [&row.name, &row.id, &row.author, &row.description]
                .iter()
                .any(|field| field.to_lowercase().contains(&query))
    }

    fn row_for(&self, id: &str) -> Option<PluginRowVm> {
        if let Some(entry) = self.entries.iter().find(|entry| entry.id == id) {
            return Some(self.installed_row(entry));
        }
        self.available
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| available_row(entry, self.external_allowed))
    }

    fn installed_row(&self, entry: &PluginEntry) -> PluginRowVm {
        let update = self.update_for(entry);
        let (primary, secondary) = self.installed_slots(entry, update.is_some());
        PluginRowVm {
            id: entry.id.clone(),
            name: entry.name.clone(),
            version: entry.version.clone(),
            author: entry.authors.join(", "),
            description: entry.description.clone(),
            badges: self.badges(entry, update),
            primary,
            secondary,
        }
    }

    fn badges(&self, entry: &PluginEntry, update: Option<String>) -> Vec<Badge> {
        let mut badges = vec![match entry.origin {
            PluginOrigin::Bundled => Badge::Bundled,
            PluginOrigin::Installed => Badge::Installed,
            PluginOrigin::Dev => Badge::Dev,
        }];
        if entry.origin != PluginOrigin::Bundled && (!entry.enabled || entry.crash_disabled) {
            badges.push(Badge::Disabled);
        }
        if let Some(reason) = &entry.refusal {
            badges.push(Badge::Refused(reason.clone()));
        }
        if let Some(version) = update {
            badges.push(Badge::UpdateAvailable(version));
        }
        badges
    }

    fn installed_slots(
        &self,
        entry: &PluginEntry,
        has_update: bool,
    ) -> (Option<Slot>, Option<Slot>) {
        if entry.origin == PluginOrigin::Bundled {
            return (None, None);
        }
        let primary = if has_update {
            Action::Update
        } else if entry.enabled {
            Action::Disable
        } else {
            Action::Enable
        };
        let blocked =
            (primary == Action::Enable && !self.external_allowed).then_some(NOT_AVAILABLE);
        (
            Some(Slot {
                action: primary,
                blocked,
            }),
            Some(Slot {
                action: Action::Remove,
                blocked: None,
            }),
        )
    }

    /// Dev extensions have no release to update from, so they never offer an update.
    fn update_for(&self, entry: &PluginEntry) -> Option<String> {
        if entry.origin != PluginOrigin::Installed {
            return None;
        }
        self.updates
            .get(&entry.id)
            .filter(|version| **version != entry.version)
            .cloned()
    }

    fn detail_for(&self, id: &str) -> Option<PluginDetail> {
        if let Some(entry) = self.entries.iter().find(|entry| entry.id == id) {
            return Some(self.entry_detail(entry));
        }
        self.available
            .iter()
            .find(|entry| entry.id == id)
            .map(available_detail)
    }

    fn entry_detail(&self, entry: &PluginEntry) -> PluginDetail {
        let mut status_lines = Vec::new();
        if entry.origin == PluginOrigin::Bundled {
            status_lines.push("Bundled with CloudDeck".to_string());
        }
        if let Some(reason) = &entry.refusal {
            status_lines.push(format!("Refused: {reason}"));
        } else if entry.origin != PluginOrigin::Bundled {
            status_lines.push(
                if entry.crash_disabled {
                    "Disabled after repeated crashes"
                } else if !entry.enabled {
                    "Disabled"
                } else {
                    "Enabled"
                }
                .to_string(),
            );
        }
        if let Some(version) = self.update_for(entry) {
            status_lines.push(format!("Update available: {version}"));
        }
        PluginDetail {
            id: entry.id.clone(),
            name: entry.name.clone(),
            version: entry.version.clone(),
            author: entry.authors.join(", "),
            description: entry.description.clone(),
            repository: entry.repository.clone(),
            platforms: platforms_label(&entry.platforms),
            capabilities: entry
                .capabilities
                .iter()
                .map(|capability| CapabilityView {
                    capability: capability.clone(),
                    granted: entry.granted.contains(capability),
                })
                .collect(),
            status_lines,
            install_path: entry.install_dir.clone(),
        }
    }
}

fn available_row(entry: &IndexEntry, external_allowed: bool) -> PluginRowVm {
    PluginRowVm {
        id: entry.id.clone(),
        name: entry.id.clone(),
        version: String::new(),
        author: String::new(),
        description: String::new(),
        badges: Vec::new(),
        primary: Some(Slot {
            action: Action::Install,
            blocked: (!external_allowed).then_some(NOT_AVAILABLE),
        }),
        secondary: None,
    }
}

fn available_detail(entry: &IndexEntry) -> PluginDetail {
    PluginDetail {
        id: entry.id.clone(),
        name: entry.id.clone(),
        version: String::new(),
        author: String::new(),
        description: String::new(),
        repository: Some(entry.repository.clone()),
        platforms: ALL_PLATFORMS.to_string(),
        capabilities: Vec::new(),
        status_lines: Vec::new(),
        install_path: None,
    }
}

fn platforms_label(platforms: &[String]) -> String {
    if platforms.is_empty() {
        ALL_PLATFORMS.to_string()
    } else {
        platforms.join(", ")
    }
}
