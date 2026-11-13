use super::*;
use clouddeck_core::fixture_fleet::{Gate, gated_provider};
use gpui::{AppContext, TestAppContext, VisualTestContext};

// clouddeck-rs-mac-ui-fix-ellipsis: the Name and ID cells of the fleet
// table are reported to render only an ellipsis ("...") in the running
// app at 1280pt. The tests build the real AppShell (the live --demo
// mount path, in both the 3-row demo and the 121-row large scenario),
// resize to the reported 1280x800, and assert the name cell keeps at
// least half the row width (and the ID cell non-zero) once the async
// demo load has landed. The headless layout measures healthy, so these
// are regression guards for the AppShell-embedded layout; a manual
// macOS --demo check at 1280pt is still the ground truth for the
// reported symptom.
fn assert_appshell_row_widths(cx: &mut VisualTestContext, row_key: &'static str) {
    let row = cx
        .debug_bounds(row_key)
        .unwrap_or_else(|| panic!("row {row_key} should render"));
    let name_key = format!("{row_key}.name");
    // `debug_bounds` only accepts `&'static str`; leak a static copy
    // (tests run in short-lived processes).
    let name_static: &'static str = Box::leak(name_key.into_boxed_str());
    let name = cx
        .debug_bounds(name_static)
        .unwrap_or_else(|| panic!("name cell {name_static} should render"));
    let id_key = format!("{row_key}.id");
    let id_static: &'static str = Box::leak(id_key.into_boxed_str());
    let id = cx
        .debug_bounds(id_static)
        .unwrap_or_else(|| panic!("id cell {id_static} should render"));

    // Name is flex_1 with a 170pt minimum; the fixed columns take the rest,
    // so a half-row share is not expected once all eight columns are shown.
    assert!(
        name.size.width >= px(170.0),
        "name cell should keep its minimum width (got {name} of {row})"
    );
    assert!(
        id.size.width > px(0.0),
        "id cell must not be zero width (got {id})"
    );
    // The regression symptom: the first column pinned to the entire row
    // width, starving the fixed Region/Type/State columns (which total
    // 240pt plus gaps/padding at this window width).
    assert!(
        name.size.width < row.size.width - px(100.0),
        "name cell must not fill the whole row (the fixed columns are \
         starved) (got {name} of {row})"
    );
}

#[gpui::test]
fn instance_row_name_and_id_cells_keep_readable_width_in_appshell_demo(cx: &mut TestAppContext) {
    let (_view, cx) = add_rooted_shell(cx, AppShell::new_demo);
    // The reported regression is at 1280pt window width.
    cx.simulate_resize(gpui::size(px(1280.0), px(800.0)));
    wait_for_row(cx, "table.row.us-east-1:i-scenario-001");

    assert_appshell_row_widths(cx, "table.row.us-east-1:i-scenario-001");
    assert_appshell_row_widths(cx, "table.row.us-east-1:i-scenario-002");
}

#[gpui::test]
fn instance_row_name_and_id_cells_keep_readable_width_in_appshell_large(cx: &mut TestAppContext) {
    let (_view, cx) = add_rooted_shell(cx, AppShell::new_demo_large);
    cx.simulate_resize(gpui::size(px(1280.0), px(800.0)));
    // 121 rows are in the model, but uniform_list only draws the visible
    // ones. Sorted by Name ascending, i-scenario-003 (bastion-east-1),
    // i-scenario-002 (batch-worker-1) and i-demo-010 (demo-10) are the
    // first three rows; i-scenario-001 (prod-api-1) sorts last.
    wait_for_row(cx, "table.row.us-east-1:i-scenario-002");

    assert_appshell_row_widths(cx, "table.row.us-east-1:i-scenario-003");
    assert_appshell_row_widths(cx, "table.row.us-east-1:i-demo-010");
}

#[gpui::test]
fn mounts_without_a_display(cx: &mut TestAppContext) {
    let (_view, cx) = add_rooted_shell(cx, AppShell::new_demo);

    assert!(
        cx.debug_bounds("app-shell").is_some(),
        "app-shell root should render"
    );
}

#[gpui::test]
fn renders_main_window_regions(cx: &mut TestAppContext) {
    let (_view, cx) = add_rooted_shell(cx, AppShell::new_demo);
    wait_for_row(cx, "table.row.us-east-1:i-scenario-001");

    assert!(
        cx.debug_bounds("main.center").is_some(),
        "the main window center should render"
    );
    assert!(
        cx.debug_bounds("main.table").is_some(),
        "the instance table should be mounted in the main window center"
    );
    assert!(
        cx.debug_bounds("main.statusbar").is_some(),
        "the main window status bar should render"
    );
}

#[gpui::test]
fn main_table_shows_the_seeded_sample_instances(cx: &mut TestAppContext) {
    let (_view, cx) = add_rooted_shell(cx, AppShell::new_demo);
    wait_for_row(cx, "table.row.us-east-1:i-scenario-001");

    assert!(
        cx.debug_bounds("main.table").is_some(),
        "the seeded sample instances should render in the main table"
    );
}

#[gpui::test]
fn startup_load_is_async_and_transitions_from_loading_to_populated(cx: &mut TestAppContext) {
    // Mirrors the startup path: the fleet entity is built and refreshed,
    // so the load is spawned instead of blocking the UI thread on
    // `cx.background_executor().block(...)`.
    let gate = Gate::new();
    let provider = gated_provider(
        clouddeck_core::fixture_fleet::instances(&[("i-gated", "", "", "")]),
        None,
        gate.clone(),
    );
    let view_model = InstanceListViewModel::new(provider);
    let fleet = cx.new(|_| FleetState::new(view_model, "scenario-default".into(), false));
    fleet.update(cx, |fleet, cx| fleet.refresh(cx));

    assert!(
        fleet.read_with(cx, |fleet, _| fleet.view_model.is_loading()),
        "the view model should report loading while the spawn is in flight"
    );
    assert!(
        fleet.read_with(cx, |fleet, _| fleet.view_model.instances().is_empty()),
        "data should not be populated synchronously while the gated load is in flight"
    );

    gate.open();
    let mut loaded = false;
    for _ in 0..200 {
        cx.run_until_parked();
        if fleet.read_with(cx, |fleet, _| {
            !fleet.view_model.is_loading() && !fleet.view_model.instances().is_empty()
        }) {
            loaded = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(loaded, "the load did not complete within the timeout");
    assert!(
        fleet.read_with(cx, |fleet, _| fleet
            .view_model
            .instances()
            .iter()
            .any(|instance| clouddeck_core::native_id(instance) == "i-gated")),
        "the gated instance should be populated once the async load completes"
    );
}

#[gpui::test]
fn main_center_and_status_bar_are_docked_in_the_window(cx: &mut TestAppContext) {
    let (_view, cx) = add_rooted_shell(cx, AppShell::new_demo);
    wait_for_row(cx, "table.row.us-east-1:i-scenario-001");

    let center = cx
        .debug_bounds("main.center")
        .expect("main center should render");
    let status_bar = cx
        .debug_bounds("main.statusbar")
        .expect("main status bar should render");

    assert!(
        status_bar.origin.y >= center.origin.y + center.size.height
            || status_bar.origin.x >= center.origin.x + center.size.width,
        "the status bar should be docked beside or below the main center"
    );
    assert!(
        status_bar.size.height > px(0.0),
        "status bar should have a non-zero height, not collapse to nothing"
    );
}

#[test]
fn app_shortcuts_use_standard_secondary_keys() {
    let bindings = app_key_bindings();
    assert_eq!(bindings.len(), 2);
    assert!(
        bindings
            .iter()
            .any(|binding| binding.action().name().ends_with("::Quit"))
    );
    assert!(
        bindings
            .iter()
            .any(|binding| binding.action().name().ends_with("::Refresh"))
    );
}

#[test]
fn demo_failure_and_partial_modes_are_observable_without_aws() {
    let context = WorkspaceContext {
        provider: "aws".into(),
        profile: "scenario-default".into(),
    };

    let fleet_regions: Vec<String> = FLEET_REGIONS.iter().map(|r| (*r).into()).collect();
    let failure = loaded_demo(&context, DemoFleet::Failure, &fleet_regions);
    assert!(
        failure
            .error_message()
            .is_some_and(|message| message.contains("Simulated demo load failure")),
        "failure scenario should fail visibly"
    );

    let partial = loaded_demo(&context, DemoFleet::Partial, &fleet_regions);
    assert_eq!(partial.instances().len(), 3);
    assert_eq!(partial.partial_failures().len(), 3);
}

fn loaded_demo(
    context: &WorkspaceContext,
    demo: DemoFleet,
    fleet_regions: &[String],
) -> InstanceListViewModel {
    let registry = clouddeck_core::extension_host::app_registry(
        None,
        std::path::Path::new("/nonexistent-clouddeck-extensions"),
        std::path::Path::new("/nonexistent-clouddeck-state"),
        vec![test_aws_provider()],
    );
    let provider = AppShell::provider_for(
        registry.providers(),
        context,
        true,
        demo,
        fleet_regions,
        Duration::from_secs(5),
    );
    let mut view_model = InstanceListViewModel::new(provider);
    crate::runtime::runtime().block_on(view_model.load_instances());
    view_model
}

#[test]
fn large_demo_mode_has_enough_rows_to_exercise_scrolling() {
    let context = WorkspaceContext {
        provider: "aws".into(),
        profile: "scenario-default".into(),
    };
    let view_model = loaded_demo(&context, DemoFleet::Large, &[]);
    assert_eq!(view_model.instances().len(), 120);
}

#[gpui::test]
fn mounting_in_real_mode_without_profiles_shows_setup_affordance(cx: &mut TestAppContext) {
    // A real (non-demo) launch with no detected AWS profiles must reach
    // the main window with an in-place "Connect to AWS" affordance (the
    // no-profiles onboarding entry point, clouddeck-rs-w6s.18) rather
    // than a dead end. The load error renders as a centered state whose
    // action is that affordance.
    let real_mode = crate::shell::is_real_mode();
    let ui_state_path = temp_ui_state_path("mount-real-mode");
    let build_path = ui_state_path.clone();
    let (shell, cx) = add_rooted_shell(cx, move |window, cx| {
        if real_mode {
            AppShell::build_with_profiles(
                window,
                false,
                DemoFleet::Happy,
                Ok(Vec::new()),
                Some(build_path),
                Settings::default(),
                cx,
            )
        } else {
            AppShell::build_with_scenario(window, true, DemoFleet::Empty, cx)
        }
    });

    wait_until(cx, "the load to settle", |cx| {
        shell.read_with(cx, |shell, cx| {
            !shell.fleet.read(cx).view_model.is_loading()
        })
    });
    let mut saw_setup = false;
    for _ in 0..200 {
        cx.run_until_parked();
        if cx.debug_bounds("center.action").is_some() {
            saw_setup = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    assert!(
        cx.debug_bounds("main.center-state").is_some(),
        "the centered state should render when the load has no instances"
    );
    if real_mode {
        assert!(
            saw_setup,
            "a real-mode launch with no profiles should show Connect to AWS"
        );
    } else {
        assert!(
            !saw_setup,
            "the demo empty scenario must not offer AWS onboarding"
        );
    }
    if let Some(dir) = ui_state_path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// The empty-state path: a shell launched with the empty-fleet demo
/// scenario settles with no instances and no error, and the centered
/// empty state renders once the load completes.
#[gpui::test]
fn empty_demo_settles_with_no_instances_and_no_error(cx: &mut TestAppContext) {
    let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo_empty);
    wait_until(cx, "the empty load to settle", |cx| {
        shell.read_with(cx, |shell, cx| {
            !shell.fleet.read(cx).view_model.is_loading()
        }) && cx.debug_bounds("main.center-state").is_some()
    });

    shell.read_with(cx, |shell, cx| {
        let vm = &shell.fleet.read(cx).view_model;
        assert!(
            vm.instances().is_empty(),
            "the empty-fleet scenario must have no instances"
        );
        assert!(
            vm.error_message().is_none(),
            "an empty but successful load must not report an error"
        );
    });
}
