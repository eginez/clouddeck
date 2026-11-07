use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use clouddeck_extension_api::API_VERSION;

use crate::about::PluginOrigin;
use crate::extension_host::{
    CRASH_LIMIT, Capability, MANIFEST_FILE, Refusal, StateStore, state_path,
};
use crate::extension_manager::index::IndexEntry;

use super::install::{Installed, Outcome};
use super::plugins::*;
use super::support::*;

#[derive(Default)]
struct Recorder {
    calls: RefCell<Vec<String>>,
}

impl Recorder {
    fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }

    fn record(&self, call: String) {
        self.calls.borrow_mut().push(call);
    }
}

impl PluginActions for Recorder {
    fn enable(&self, id: &str) -> Result<(), String> {
        self.record(format!("enable {id}"));
        Ok(())
    }

    fn disable(&self, id: &str) -> Result<(), String> {
        self.record(format!("disable {id}"));
        Ok(())
    }

    fn remove(&self, id: &str) -> Result<(), String> {
        self.record(format!("remove {id}"));
        Ok(())
    }

    fn install(&self, id: &str) -> Result<Outcome, String> {
        self.record(format!("install {id}"));
        Ok(Outcome::Installed(installed(id)))
    }

    fn update(&self, id: &str) -> Result<Outcome, String> {
        self.record(format!("update {id}"));
        Ok(Outcome::UpToDate {
            version: "0.1.0".into(),
        })
    }

    fn install_dev(&self, path: &Path) -> Result<Outcome, String> {
        self.record(format!("install_dev {}", path.display()));
        Ok(Outcome::Installed(installed("dev-one")))
    }
}

fn installed(id: &str) -> Installed {
    Installed {
        id: id.into(),
        version: "0.1.0".into(),
        repository: None,
        author_key: None,
        capabilities: Vec::new(),
    }
}

fn entry(id: &str, origin: PluginOrigin) -> PluginEntry {
    PluginEntry {
        id: id.into(),
        name: format!("{id} cloud"),
        version: "0.1.0".into(),
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

fn index_entry(id: &str) -> IndexEntry {
    IndexEntry {
        id: id.into(),
        repository: format!("https://github.com/eginez/clouddeck-{id}"),
        author_key: "AAAA".into(),
    }
}

fn ids(model: &PluginsViewModel) -> Vec<String> {
    model.rows.iter().map(|row| row.id.clone()).collect()
}

fn row<'a>(model: &'a PluginsViewModel, id: &str) -> &'a PluginRowVm {
    model.rows.iter().find(|row| row.id == id).unwrap()
}

fn write_plugin(root: &Path, id: &str, text: &str) {
    let dir = root.join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(MANIFEST_FILE), text).unwrap();
}

fn manifest_text(id: &str, extra: &str) -> String {
    format!(
        "id = \"{id}\"\nname = \"{id} cloud\"\nversion = \"0.1.0\"\nschema_version = 1\napi_version = {API_VERSION}\n{extra}\n[providers.{id}]\nname = \"{id} cloud\"\n"
    )
}

#[test]
fn search_matches_name_id_author_and_description() {
    let mut toy = entry("toy", PluginOrigin::Installed);
    toy.authors = vec!["Ada Lovelace".into()];
    toy.description = "Lists queues".into();
    let quiet = entry("quiet", PluginOrigin::Installed);
    let mut model = PluginsViewModel::new(vec![toy, quiet], vec![], BTreeMap::new(), false);

    assert_eq!(ids(&model), ["quiet", "toy"]);
    model.set_query("TOY");
    assert_eq!(ids(&model), ["toy"]);
    model.set_query("ada");
    assert_eq!(ids(&model), ["toy"]);
    model.set_query("queues");
    assert_eq!(ids(&model), ["toy"]);
    model.set_query("quiet");
    assert_eq!(ids(&model), ["quiet"]);
    model.set_query("");
    assert_eq!(ids(&model), ["quiet", "toy"]);
}

#[test]
fn installed_tab_lists_bundled_installed_and_dev_while_available_drops_installed() {
    let demo = entry("demo", PluginOrigin::Bundled);
    let toy = entry("toy", PluginOrigin::Installed);
    let available = vec![index_entry("toy"), index_entry("other")];
    let mut model = PluginsViewModel::new(vec![demo, toy], available, BTreeMap::new(), false);

    assert_eq!(ids(&model), ["demo", "toy"]);
    model.set_tab(Tab::Available);
    assert_eq!(ids(&model), ["other"]);
    assert_eq!(
        row(&model, "other").primary.as_ref().unwrap().action,
        Action::Install
    );
}

#[test]
fn badges_follow_state_and_order() {
    let installed = entry("a", PluginOrigin::Installed);
    let mut disabled = entry("b", PluginOrigin::Installed);
    disabled.enabled = false;
    let dev = entry("c", PluginOrigin::Dev);
    let mut refused = entry("d", PluginOrigin::Installed);
    refused.refusal = Some("built for API 3".into());
    let update = entry("e", PluginOrigin::Installed);
    let mut crashed = entry("f", PluginOrigin::Installed);
    crashed.crash_disabled = true;
    let mut everything = entry("g", PluginOrigin::Installed);
    everything.enabled = false;
    everything.refusal = Some("built for API 3".into());
    let updates = BTreeMap::from([
        ("c".to_string(), "0.2.0".to_string()),
        ("e".to_string(), "0.2.0".to_string()),
        ("g".to_string(), "0.2.0".to_string()),
    ]);
    let model = PluginsViewModel::new(
        vec![
            installed, disabled, dev, refused, update, crashed, everything,
        ],
        vec![],
        updates,
        false,
    );

    assert_eq!(row(&model, "a").badges, [Badge::Installed]);
    assert_eq!(row(&model, "b").badges, [Badge::Installed, Badge::Disabled]);
    assert_eq!(row(&model, "c").badges, [Badge::Dev]);
    assert_eq!(
        row(&model, "d").badges,
        [Badge::Installed, Badge::Refused("built for API 3".into())]
    );
    assert_eq!(
        row(&model, "e").badges,
        [Badge::Installed, Badge::UpdateAvailable("0.2.0".into())]
    );
    assert_eq!(row(&model, "f").badges, [Badge::Installed, Badge::Disabled]);
    assert_eq!(
        row(&model, "g").badges,
        [
            Badge::Installed,
            Badge::Disabled,
            Badge::Refused("built for API 3".into()),
            Badge::UpdateAvailable("0.2.0".into()),
        ]
    );
}

#[test]
fn bundled_plugins_have_no_actions_and_never_a_disabled_badge() {
    let mut aws = entry("aws", PluginOrigin::Bundled);
    aws.enabled = false;
    aws.crash_disabled = true;
    let recorder = Recorder::default();
    let mut model = PluginsViewModel::new(vec![aws], vec![], BTreeMap::new(), true);

    let aws_row = row(&model, "aws");
    assert_eq!(aws_row.badges, [Badge::Bundled]);
    assert_eq!(aws_row.primary, None);
    assert_eq!(aws_row.secondary, None);
    assert!(model.act(&recorder, "aws", Action::Remove).is_err());
    assert!(model.act(&recorder, "aws", Action::Disable).is_err());
    assert!(recorder.calls().is_empty());
}

#[test]
fn restart_required_after_disable() {
    let fx = fixture();
    fx.install_v1();
    let manager = fx.manager();
    manager.enable_with("toy", true).unwrap();
    let entries = load_entries(None, &fx.root, &fx.state_root, &[]).unwrap();
    let mut model = PluginsViewModel::new(entries, vec![], BTreeMap::new(), true);

    assert_eq!(
        model.act(&manager, "toy", Action::Disable).unwrap(),
        "Disabled Toy. Restart CloudDeck to apply."
    );
    assert_eq!(model.restart_required(), ["toy"]);
    assert!(!fx.state_enabled());
}

#[test]
fn restart_required_after_remove() {
    let fx = fixture();
    fx.install_v1();
    let manager = fx.manager();
    let entries = load_entries(None, &fx.root, &fx.state_root, &[]).unwrap();
    let mut model = PluginsViewModel::new(entries, vec![], BTreeMap::new(), true);

    assert!(model.act(&manager, "demo", Action::Remove).is_err());
    model.act(&manager, "toy", Action::Remove).unwrap();
    assert_eq!(model.restart_required(), ["toy"]);
    assert!(!fx.root.join("toy").exists());
}

#[test]
fn restart_required_after_update() {
    let fx = fixture();
    fx.install_v1();
    fx.publish_release(
        &fx.author.0,
        "0.2.0",
        &standard_archive("toy", API_VERSION, "0.2.0"),
    );
    let entries = load_entries(None, &fx.root, &fx.state_root, &[]).unwrap();
    let updates = BTreeMap::from([("toy".to_string(), "0.2.0".to_string())]);
    let mut model = PluginsViewModel::new(entries, vec![], updates, true);

    assert_eq!(
        model.act(&fx.manager(), "toy", Action::Update).unwrap(),
        "Updated Toy. Restart CloudDeck to apply."
    );
    assert_eq!(model.restart_required(), ["toy"]);
}

#[test]
fn restart_required_after_install_dev() {
    let fx = fixture();
    let source = tempfile::tempdir().unwrap();
    fs::write(
        source.path().join(MANIFEST_FILE),
        extension_toml("dev-one", API_VERSION, "0.1.0"),
    )
    .unwrap();
    fs::write(source.path().join(library_name("dev_one")), b"fake").unwrap();
    let mut model = PluginsViewModel::new(vec![], vec![], BTreeMap::new(), true);

    assert_eq!(
        model.install_dev(&fx.manager(), source.path()).unwrap(),
        "Installed dev-one. Restart CloudDeck to apply."
    );
    assert_eq!(model.restart_required(), ["dev-one"]);
}

#[test]
fn gate_off_blocks_enable_and_install_without_calling_the_manager() {
    let mut disabled = entry("x", PluginOrigin::Installed);
    disabled.enabled = false;
    let recorder = Recorder::default();
    let mut model = PluginsViewModel::new(
        vec![disabled],
        vec![index_entry("toy")],
        BTreeMap::new(),
        false,
    );

    assert_eq!(
        row(&model, "x").primary.as_ref().unwrap().blocked,
        Some(NOT_AVAILABLE)
    );
    assert_eq!(
        model.act(&recorder, "x", Action::Enable),
        Err(NOT_AVAILABLE.to_string())
    );
    model.set_tab(Tab::Available);
    assert_eq!(
        row(&model, "toy").primary.as_ref().unwrap().blocked,
        Some(NOT_AVAILABLE)
    );
    assert_eq!(
        model.act(&recorder, "toy", Action::Install),
        Err(NOT_AVAILABLE.to_string())
    );
    assert!(recorder.calls().is_empty());
}

#[test]
fn gate_off_still_allows_disable_and_gate_on_calls_the_manager() {
    let enabled = entry("x", PluginOrigin::Installed);
    let recorder = Recorder::default();
    let mut closed = PluginsViewModel::new(vec![enabled.clone()], vec![], BTreeMap::new(), false);
    assert!(closed.act(&recorder, "x", Action::Disable).is_ok());

    let mut open = PluginsViewModel::new(vec![enabled], vec![], BTreeMap::new(), true);
    assert!(open.act(&recorder, "x", Action::Disable).is_ok());
    assert_eq!(recorder.calls(), ["disable x", "disable x"]);
}

#[test]
fn selection_clears_when_its_row_leaves_the_list() {
    let mut model = PluginsViewModel::new(
        vec![entry("toy", PluginOrigin::Installed)],
        vec![],
        BTreeMap::new(),
        false,
    );
    model.select(Some("toy"));
    assert!(model.detail.is_some());

    model.set_query("zzz");
    assert_eq!(model.selected, None);
    assert_eq!(model.detail, None);
}

#[test]
fn empty_available_tab_explains_that_no_index_exists() {
    let mut model = PluginsViewModel::new(
        vec![entry("toy", PluginOrigin::Installed)],
        vec![],
        BTreeMap::new(),
        false,
    );
    model.set_tab(Tab::Available);

    assert!(model.rows.is_empty());
    assert_eq!(model.empty_state(), Some(AVAILABLE_EMPTY));
}

#[test]
fn empty_search_says_no_plugins_match() {
    let mut model = PluginsViewModel::new(
        vec![entry("toy", PluginOrigin::Installed)],
        vec![],
        BTreeMap::new(),
        false,
    );

    model.set_query("zzz");
    assert!(model.rows.is_empty());
    assert_eq!(model.empty_state(), Some(NO_MATCHES));
    model.set_query("");
    assert_eq!(model.empty_state(), None);
}

#[test]
fn detail_status_lines_cover_crash_refusal_update_and_bundled() {
    let mut crashed = entry("a", PluginOrigin::Installed);
    crashed.crash_disabled = true;
    let mut refused = entry("b", PluginOrigin::Installed);
    refused.refusal = Some("built for API 3".into());
    let updated = entry("c", PluginOrigin::Installed);
    let mut off = entry("d", PluginOrigin::Installed);
    off.enabled = false;
    let aws = entry("aws", PluginOrigin::Bundled);
    let updates = BTreeMap::from([("c".to_string(), "0.2.0".to_string())]);
    let mut model = PluginsViewModel::new(
        vec![crashed, refused, updated, off, aws],
        vec![],
        updates,
        false,
    );

    let lines = |model: &mut PluginsViewModel, id: &str| {
        model.select(Some(id));
        model.detail.clone().unwrap().status_lines
    };
    assert_eq!(lines(&mut model, "a"), ["Disabled after repeated crashes"]);
    assert_eq!(lines(&mut model, "b"), ["Refused: built for API 3"]);
    assert_eq!(
        lines(&mut model, "c"),
        ["Enabled", "Update available: 0.2.0"]
    );
    assert_eq!(lines(&mut model, "d"), ["Disabled"]);
    assert_eq!(lines(&mut model, "aws"), ["Bundled with CloudDeck"]);
}

#[test]
fn detail_shows_granted_capabilities_and_platforms() {
    let mut toy = entry("toy", PluginOrigin::Installed);
    toy.capabilities = vec![Capability::Credentials, Capability::Secrets];
    toy.granted = vec![Capability::Credentials];
    toy.platforms = vec!["macos".into(), "linux".into()];
    let mut model = PluginsViewModel::new(vec![toy], vec![], BTreeMap::new(), false);
    model.select(Some("toy"));
    let detail = model.detail.clone().unwrap();

    assert_eq!(detail.platforms, "macos, linux");
    assert_eq!(
        detail.capabilities,
        [
            CapabilityView {
                capability: Capability::Credentials,
                granted: true,
            },
            CapabilityView {
                capability: Capability::Secrets,
                granted: false,
            },
        ]
    );
}

#[test]
fn loader_reads_manifests_state_and_refusals() {
    let fx = fixture();
    fs::create_dir_all(&fx.state_root).unwrap();
    write_plugin(
        &fx.root,
        "toy",
        &manifest_text(
            "toy",
            "authors = [\"Ada\"]\ndescription = \"Lists queues\"\nplatforms = [\"linux\"]\n",
        ),
    );
    write_plugin(&fx.root, "dev-one", &manifest_text("dev-one", ""));
    StateStore::open(&state_path(&fx.state_root, "dev-one"))
        .unwrap()
        .update(|state| {
            state.dev = true;
            state.enabled = true;
        })
        .unwrap();
    write_plugin(&fx.root, "old", &manifest_text("old", ""));
    write_plugin(&fx.root, "crashy", &manifest_text("crashy", ""));
    StateStore::open(&state_path(&fx.state_root, "crashy"))
        .unwrap()
        .update(|state| {
            state.enabled = true;
            state.crashes = CRASH_LIMIT;
        })
        .unwrap();
    let refusals = vec![Refusal {
        extension: "old".into(),
        message: "built for API 0".into(),
    }];

    let entries = load_entries(None, &fx.root, &fx.state_root, &refusals).unwrap();
    let find = |id: &str| entries.iter().find(|entry| entry.id == id).unwrap();

    assert_eq!(find("demo").origin, PluginOrigin::Bundled);
    assert!(find("demo").enabled);
    assert_eq!(find("toy").origin, PluginOrigin::Installed);
    assert_eq!(find("toy").authors, ["Ada"]);
    assert_eq!(find("toy").description, "Lists queues");
    assert_eq!(find("toy").platforms, ["linux"]);
    assert!(!find("toy").enabled);
    assert_eq!(find("dev-one").origin, PluginOrigin::Dev);
    assert_eq!(find("old").refusal.as_deref(), Some("built for API 0"));
    assert!(find("crashy").crash_disabled);
}
