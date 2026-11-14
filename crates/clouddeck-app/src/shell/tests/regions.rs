use super::*;
use gpui::{TestAppContext, VisualTestContext};
use std::collections::HashSet;

fn recording_aws(
    regions: Vec<String>,
) -> (
    std::sync::Arc<dyn Provider>,
    std::sync::Arc<std::sync::Mutex<Vec<String>>>,
) {
    let queried = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = queried.clone();
    let provider = clouddeck_core::ComputeServiceProvider::new(
        "aws",
        vec![clouddeck_core::Connection {
            id: "default".into(),
            display_name: "default".to_string(),
            detail: None,
            tags: Vec::new(),
        }],
        regions,
        move |_, region| {
            sink.lock().unwrap().push(region.to_string());
            Box::new(clouddeck_core::MockComputeService::new(
                vec![bare_ec2(region, &format!("i-{region}"))],
                None,
            ))
        },
    );
    (std::sync::Arc::new(provider), queried)
}

fn sorted_queries(queried: &std::sync::Mutex<Vec<String>>) -> Vec<String> {
    let mut regions = queried.lock().unwrap().clone();
    regions.sort();
    regions.dedup();
    regions
}

fn region_dimension_values(shell: &Entity<AppShell>, cx: &mut VisualTestContext) -> Vec<String> {
    shell.read_with(cx, |shell, _| {
        shell
            .context_model
            .dimensions()
            .iter()
            .find(|dim| dim.id.as_str() == REGION)
            .map(|dim| {
                dim.values
                    .iter()
                    .map(|v| v.id.as_str().to_string())
                    .collect()
            })
            .unwrap_or_default()
    })
}

fn aws_regions() -> Vec<String> {
    clouddeck_core::REGIONS
        .iter()
        .map(|region| (*region).to_string())
        .collect()
}

fn shell_with_ui_state(
    cx: &mut TestAppContext,
    aws: std::sync::Arc<dyn Provider>,
    ui_file: Option<std::path::PathBuf>,
) -> (Entity<AppShell>, &mut VisualTestContext) {
    install_providers(cx, vec![aws]);
    add_rooted_shell(cx, move |window, cx| {
        AppShell::build_with_profiles(
            window,
            false,
            DemoFleet::Happy,
            Ok(Vec::new()),
            ui_file,
            Settings::default(),
            cx,
        )
    })
}

fn shell_restored_from_ui_file<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    contents: &str,
) -> (Entity<AppShell>, &'a mut VisualTestContext) {
    let ui_file = std::env::temp_dir().join(format!("{name}-{}.json", std::process::id()));
    std::fs::write(&ui_file, contents).expect("write ui state");
    let (aws, _) = recording_aws(aws_regions());
    let (shell, cx) = shell_with_ui_state(cx, aws, Some(ui_file));
    cx.run_until_parked();
    (shell, cx)
}

#[gpui::test]
fn switching_provider_reloads_the_context_dimensions(cx: &mut TestAppContext) {
    let (aws, _) = recording_aws(vec!["us-east-1".into(), "eu-west-1".into()]);
    let gcp: std::sync::Arc<dyn Provider> =
        std::sync::Arc::new(clouddeck_core::ComputeServiceProvider::new(
            "gcp",
            vec![clouddeck_core::Connection {
                id: "default".into(),
                display_name: "default".to_string(),
                detail: None,
                tags: Vec::new(),
            }],
            vec!["europe-west1".into()],
            |_, _| Box::new(clouddeck_core::MockComputeService::new(Vec::new(), None)),
        ));
    install_providers(cx, vec![aws, gcp]);
    let (shell, cx) = add_rooted_shell(cx, |window, cx| {
        AppShell::build_with_profiles(
            window,
            true,
            DemoFleet::Happy,
            Ok(Vec::new()),
            None,
            Settings::default(),
            cx,
        )
    });
    assert_eq!(
        region_dimension_values(&shell, cx),
        ["us-east-1", "eu-west-1"]
    );

    let settings_file =
        std::env::temp_dir().join(format!("clouddeck-dimensions-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&settings_file);
    shell.update_in(cx, |shell, window, cx| {
        shell.switch_provider_to("gcp", &settings_file, window, cx)
    });
    cx.run_until_parked();

    assert_eq!(region_dimension_values(&shell, cx), ["europe-west1"]);
    let _ = std::fs::remove_file(&settings_file);
}

#[gpui::test]
fn partial_region_selection_reaches_the_listing_call(cx: &mut TestAppContext) {
    let ui_file =
        std::env::temp_dir().join(format!("clouddeck-selection-{}.json", std::process::id()));
    UiState {
        dimension_selections: BTreeMap::from([(
            REGION.to_string(),
            vec!["us-west-2".into(), "eu-west-1".into()],
        )]),
        ..UiState::default()
    }
    .save(&ui_file)
    .expect("save ui state");
    let (aws, queried) = recording_aws(aws_regions());
    let (shell, cx) = shell_with_ui_state(cx, aws, Some(ui_file));
    cx.run_until_parked();

    assert_eq!(sorted_queries(&queried), ["eu-west-1", "us-west-2"]);
    shell.read_with(cx, |shell, _| {
        assert_eq!(
            shell.context_model.selection().values(REGION),
            [ValueId::from("us-west-2"), ValueId::from("eu-west-1")]
        );
    });
}

#[gpui::test]
fn fleet_selection_queries_its_regions_and_counts_loaded_instances(cx: &mut TestAppContext) {
    let ui_file = std::env::temp_dir().join(format!(
        "clouddeck-fleet-selection-{}.json",
        std::process::id()
    ));
    UiState {
        dimension_selections: BTreeMap::from([(
            REGION.to_string(),
            FLEET_REGIONS.iter().map(|r| r.to_string()).collect(),
        )]),
        ..UiState::default()
    }
    .save(&ui_file)
    .expect("save ui state");
    let (aws, queried) = recording_aws(aws_regions());
    let (shell, cx) = shell_with_ui_state(cx, aws, Some(ui_file));
    cx.run_until_parked();

    let mut expected: Vec<String> = FLEET_REGIONS.iter().map(|r| r.to_string()).collect();
    expected.sort();
    assert_eq!(sorted_queries(&queried), expected);
    shell.read_with(cx, |shell, _| {
        assert_eq!(
            shell
                .context_model
                .value_count(REGION, &ValueId::from("us-west-2")),
            1
        );
    });
}

#[test]
fn fleet_regions_follow_the_region_selection() {
    let available = AppShell::available_regions(false, DemoFleet::Happy);
    let mut model = ContextModel::new(Vec::new(), None, "aws");
    ensure_region_dimension(&mut model, &available);
    assert_eq!(AppShell::fleet_regions_for(&model), FLEET_REGIONS);
    set_region_values(&mut model, &["eu-west-1".to_string()]);
    assert_eq!(AppShell::fleet_regions_for(&model), ["eu-west-1"]);
}

#[test]
fn demo_region_universe_extends_the_demo_list_with_every_aws_region() {
    for scenario in [
        DemoFleet::Happy,
        DemoFleet::Empty,
        DemoFleet::Failure,
        DemoFleet::Partial,
        DemoFleet::Large,
        DemoFleet::Huge,
    ] {
        let demo_list: Vec<String> = AppShell::regions(true, scenario)
            .into_iter()
            .filter(|region| region != "fleet")
            .collect();
        let universe = AppShell::available_regions(true, scenario);
        assert_eq!(&universe[..demo_list.len()], demo_list.as_slice());
        for region in clouddeck_core::REGIONS
            .iter()
            .filter(|region| **region != "fleet")
        {
            assert!(universe.iter().any(|known| known == region));
        }
        assert!(!universe.iter().any(|region| region == "fleet"));
        let unique: HashSet<&String> = universe.iter().collect();
        assert_eq!(unique.len(), universe.len());
    }
}

#[gpui::test]
fn all_regions_are_not_saved_in_dimension_selections(cx: &mut TestAppContext) {
    let (aws, _) = recording_aws(aws_regions());
    let (shell, cx) = shell_with_ui_state(cx, aws, None);
    shell.update(cx, |shell, cx| {
        shell.context_model.select_all(REGION);
        shell.remembered_regions = Some(AppShell::remembered_regions_for(&shell.context_model));
        assert!(
            !shell
                .ui_state_snapshot(cx)
                .dimension_selections
                .contains_key(REGION)
        );
    });
}

#[gpui::test]
fn legacy_selected_regions_migrate_once_into_the_region_selection(cx: &mut TestAppContext) {
    let ui_file = std::env::temp_dir().join(format!(
        "clouddeck-legacy-selection-{}.json",
        std::process::id()
    ));
    std::fs::write(
        &ui_file,
        r#"{"selected_regions":["us-west-2","eu-west-1"]}"#,
    )
    .expect("write legacy ui state");
    let (aws, queried) = recording_aws(aws_regions());
    let (shell, cx) = shell_with_ui_state(cx, aws, Some(ui_file));
    cx.run_until_parked();

    assert_eq!(sorted_queries(&queried), ["eu-west-1", "us-west-2"]);
    shell.read_with(cx, |shell, _| {
        assert_eq!(
            region_values(&shell.context_model),
            ["us-west-2", "eu-west-1"]
        );
        assert!(shell.ui_state_saved.selected_regions.is_none());
    });
    shell.read_with(cx, |shell, cx| {
        let path = shell.ui_state_path.clone().expect("ui state path");
        shell
            .ui_state_snapshot(cx)
            .save(&path)
            .expect("save ui state");
        let saved = std::fs::read_to_string(&path).expect("read saved ui state");
        assert!(!saved.contains("selected_regions"));
        let _ = std::fs::remove_file(&path);
    });
}

#[gpui::test]
fn stored_region_selection_wins_over_the_legacy_key(cx: &mut TestAppContext) {
    let ui_file = std::env::temp_dir().join(format!(
        "clouddeck-stored-selection-{}.json",
        std::process::id()
    ));
    std::fs::write(
        &ui_file,
        r#"{"dimension_selections":{"region":["us-west-2"]},"selected_regions":["eu-west-1"]}"#,
    )
    .expect("write ui state");
    let (aws, _) = recording_aws(aws_regions());
    let (shell, cx) = shell_with_ui_state(cx, aws, Some(ui_file));
    cx.run_until_parked();

    shell.read_with(cx, |shell, _| {
        assert_eq!(region_values(&shell.context_model), ["us-west-2"]);
    });
}

#[gpui::test]
fn remembered_all_regions_restore_as_all_regions(cx: &mut TestAppContext) {
    let (shell, cx) =
        shell_restored_from_ui_file(cx, "clouddeck-remembered-all", r#"{"sidebar_open":true}"#);
    shell.read_with(cx, |shell, _| {
        assert!(shell.context_model.is_all(REGION));
    });
}

#[gpui::test]
fn unreadable_ui_state_restores_the_default_regions_not_all(cx: &mut TestAppContext) {
    let (shell, cx) = shell_restored_from_ui_file(cx, "clouddeck-unreadable-ui", "{ not json");
    shell.read_with(cx, |shell, _| {
        assert!(!shell.context_model.is_all(REGION));
    });
}

#[test]
fn demo_regions_only_include_fixture_regions_and_fleet() {
    assert_eq!(
        AppShell::regions(true, DemoFleet::Happy),
        vec!["fleet".to_string(), "us-east-1".to_string()]
    );
    assert!(
        AppShell::regions(true, DemoFleet::Happy)
            .iter()
            .all(|region| region == "fleet" || region == "us-east-1")
    );
}

#[gpui::test]
fn region_scope_limits_the_fleet_query(cx: &mut TestAppContext) {
    let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo_partial);
    wait_until_context_settled(cx, &shell);

    let selected = vec!["us-east-1".to_string(), "eu-west-1".to_string()];
    emit_main_window_event(cx, &shell, MainWindowEvent::RegionsChanged(selected));
    wait_until_context_settled(cx, &shell);

    shell.read_with(cx, |shell, cx| {
        let vm = &shell.fleet.read(cx).view_model;
        assert_eq!(vm.partial_failures().len(), 1);
        assert_eq!(vm.partial_failures()[0].region, "eu-west-1");
        assert!(
            vm.instances()
                .iter()
                .all(|instance| instance.region.as_deref() == Some("us-east-1"))
        );
    });
}
