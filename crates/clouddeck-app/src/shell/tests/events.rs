use super::*;
use clouddeck_core::fixture_fleet::{Gate, gated_provider};
use gpui::TestAppContext;

#[gpui::test]
fn a_superseded_context_load_does_not_overwrite_the_current_context(cx: &mut TestAppContext) {
    let gate = Gate::new();
    let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);
    wait_for_row(cx, "table.row.us-east-1:i-scenario-001");

    shell.update(cx, |shell, _| {
        shell.set_test_service_override(gated_provider(vec![], None, gate.clone()));
    });
    emit_main_window_event(
        cx,
        &shell,
        MainWindowEvent::RegionsChanged(vec!["us-west-2".into()]),
    );

    shell.update(cx, |shell, _| shell.test_service_override = None);
    emit_main_window_event(
        cx,
        &shell,
        MainWindowEvent::RegionsChanged(vec!["us-east-1".into()]),
    );
    wait_until(cx, "the current context load to settle", |cx| {
        shell.read_with(cx, |shell, cx| {
            !shell.fleet.read(cx).view_model.is_loading()
        })
    });

    gate.open();
    for _ in 0..20 {
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    assert!(
        cx.debug_bounds("table.row.us-east-1:i-scenario-001")
            .is_some(),
        "the superseded load must not clear the current context's rows"
    );
}

fn fleet_error(cx: &mut gpui::VisualTestContext, shell: &gpui::Entity<AppShell>) -> Option<String> {
    shell.read_with(cx, |shell, cx| {
        shell
            .fleet
            .read(cx)
            .view_model
            .error_message()
            .map(str::to_owned)
    })
}

fn all_regions_event(
    shell: &gpui::Entity<AppShell>,
    cx: &mut gpui::VisualTestContext,
) -> MainWindowEvent {
    MainWindowEvent::RegionsChanged(shell.read_with(cx, |shell, _| {
        AppShell::available_regions(shell.demo, shell.demo_scenario)
    }))
}

/// A region change from the main window's region overlay must drive the
/// shell: the context updates and the fleet reloads for the selected
/// region.
#[gpui::test]
fn region_change_event_drives_shell_context_and_fleet_reload(cx: &mut TestAppContext) {
    let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);
    wait_for_row(cx, "table.row.us-east-1:i-scenario-001");

    let all_regions = shell.read_with(cx, |shell, _| {
        AppShell::available_regions(shell.demo, shell.demo_scenario)
    });
    emit_main_window_event(cx, &shell, MainWindowEvent::RegionsChanged(all_regions));

    shell.read_with(cx, |shell, _| {
        assert!(
            shell.context_model.is_all(REGION),
            "the shell's region selection must track the region overlay's selection"
        );
    });

    wait_until_context_settled(cx, &shell);
}

/// Switching regions must clear the previously selected instance, since
/// the fleet state is reset for the new context.
#[gpui::test]
fn region_change_event_clears_the_selected_instance(cx: &mut TestAppContext) {
    let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);
    wait_for_row(cx, "table.row.us-east-1:i-scenario-001");

    let row = cx
        .debug_bounds("table.row.us-east-1:i-scenario-001")
        .expect("a seeded instance row should render after the startup load");
    cx.simulate_click(row.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    assert!(
        shell.read_with(cx, |shell, cx| shell.fleet.read(cx).selected.is_some()),
        "clicking a row should select it"
    );

    let all_regions = shell.read_with(cx, |shell, _| {
        AppShell::available_regions(shell.demo, shell.demo_scenario)
    });
    emit_main_window_event(cx, &shell, MainWindowEvent::RegionsChanged(all_regions));

    let (is_all, selection) = shell.read_with(cx, |shell, cx| {
        (
            shell.context_model.is_all(REGION),
            shell.fleet.read(cx).selected.clone(),
        )
    });
    assert!(
        is_all,
        "the shell's region selection must track the selection"
    );
    assert!(
        selection.is_none(),
        "the old selection must not survive a context switch"
    );

    wait_until_context_settled(cx, &shell);
}

/// The switch-profile action opens the profile overlay, and escape closes
/// it without changing the context.
#[gpui::test]
fn switch_profile_action_opens_and_escape_dismisses_without_changing_context(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);
    wait_for_row(cx, "table.row.us-east-1:i-scenario-001");

    let context_before = shell.read_with(cx, |shell, _| shell.context.clone());

    cx.dispatch_action(crate::app_menus::SwitchProfile);
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("main.overlay.profile").is_some(),
        "the switch-profile action should open the profile overlay"
    );

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("main.overlay.profile").is_none(),
        "escape should close the profile overlay"
    );

    let context_after = shell.read_with(cx, |shell, _| shell.context.clone());
    assert_eq!(
        context_after, context_before,
        "dismissing the profile overlay must not change the context"
    );
}

/// Selecting a different profile must update the shell's context and
/// reload the fleet for the new profile.
#[gpui::test]
fn pick_profile_event_drives_shell_context(cx: &mut TestAppContext) {
    let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);
    wait_for_row(cx, "table.row.us-east-1:i-scenario-001");

    emit_main_window_event(
        cx,
        &shell,
        MainWindowEvent::PickProfile("other-profile".into()),
    );

    let context_profile = shell.read_with(cx, |shell, _| shell.context.profile.clone());
    assert_eq!(
        context_profile, "other-profile",
        "the shell's context profile must match the picked profile"
    );

    // The demo service for an unknown profile returns no instances and no error.
    wait_until(cx, "profile switch load to settle", |cx| {
        shell.read_with(cx, |shell, cx| {
            let vm = &shell.fleet.read(cx).view_model;
            !vm.is_loading() && vm.instances().is_empty()
        })
    });
    assert!(
        fleet_error(cx, &shell).is_none(),
        "the demo service for an unknown profile should not produce an error"
    );
}

/// A region change on a failure-scenario shell must settle with the
/// simulated load error on the fleet.
#[gpui::test]
fn region_change_on_failure_shell_settles_with_the_simulated_error(cx: &mut TestAppContext) {
    let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo_failure);
    wait_until(cx, "error to render after startup", |cx| {
        fleet_error(cx, &shell).is_some()
    });

    assert!(
        shell.read_with(cx, |shell, _| !shell.context_model.is_all(REGION)),
        "the test must start in a concrete region so the fleet switch is a meaningful change"
    );

    let event = all_regions_event(&shell, cx);
    emit_main_window_event(cx, &shell, event);

    assert!(
        shell.read_with(cx, |shell, _| shell.context_model.is_all(REGION)),
        "the shell's region selection must track the region overlay's selection"
    );

    wait_until(cx, "the switched failure load to settle", |cx| {
        fleet_error(cx, &shell).is_some()
    });
    let error_msg = fleet_error(cx, &shell);
    assert!(
        error_msg
            .as_deref()
            .unwrap_or("")
            .contains("Simulated demo load failure"),
        "the error message should be the simulated demo failure, got: {error_msg:?}"
    );
}

/// A region change with a gated service injected into `apply_context`
/// must show the fleet loading while the load is in flight, then the
/// simulated error once the gate is released.
#[gpui::test]
fn region_change_shows_loading_state_during_failure_load(cx: &mut TestAppContext) {
    let gate = Gate::new();
    let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo_failure);
    wait_until(cx, "error to render after startup", |cx| {
        fleet_error(cx, &shell).is_some()
    });

    assert!(
        shell.read_with(cx, |shell, _| !shell.context_model.is_all(REGION)),
        "the test must start in a concrete region so the fleet switch is a meaningful change"
    );

    shell.update(cx, |shell, _| {
        shell.set_test_service_override(gated_provider(
            vec![],
            Some("Simulated demo load failure".to_string()),
            gate.clone(),
        ));
    });

    let event = all_regions_event(&shell, cx);
    emit_main_window_event(cx, &shell, event);

    let (is_all, is_loading) = shell.read_with(cx, |shell, cx| {
        (
            shell.context_model.is_all(REGION),
            shell.fleet.read(cx).view_model.is_loading(),
        )
    });
    assert!(
        is_all,
        "the shell's region selection must track the region overlay's selection"
    );
    assert!(
        is_loading,
        "the fleet must report loading while the gated load is in flight"
    );
    assert!(
        fleet_error(cx, &shell).is_none(),
        "the error state must not be visible before the gate is released"
    );

    gate.open();
    wait_until(cx, "the gated failure load to settle", |cx| {
        fleet_error(cx, &shell).is_some()
    });

    let (is_loading_after, error_msg) = shell.read_with(cx, |shell, cx| {
        (
            shell.fleet.read(cx).view_model.is_loading(),
            shell
                .fleet
                .read(cx)
                .view_model
                .error_message()
                .map(str::to_owned),
        )
    });
    assert!(
        !is_loading_after,
        "the view model must no longer be loading after the gated load settles"
    );
    assert!(
        error_msg
            .as_deref()
            .unwrap_or("")
            .contains("Simulated demo load failure"),
        "the error message should be the simulated demo failure, got: {error_msg:?}"
    );
}

#[gpui::test]
fn applying_a_new_demo_context_updates_context_and_clears_old_rows(cx: &mut TestAppContext) {
    let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);
    wait_for_row(cx, "table.row.us-east-1:i-scenario-001");

    shell.update_in(cx, |shell, window, cx| {
        set_region_values(&mut shell.context_model, &["us-west-2".to_string()]);
        shell.apply_context(
            WorkspaceContext {
                provider: "aws".into(),
                profile: "scenario-default".into(),
            },
            window,
            cx,
        );
    });

    wait_until(cx, "the new context load to settle", |cx| {
        shell.read_with(cx, |shell, cx| {
            !shell.fleet.read(cx).view_model.is_loading()
        })
    });
    shell.read_with(cx, |shell, cx| {
        assert_eq!(region_values(&shell.context_model), ["us-west-2"]);
        assert!(shell.fleet.read(cx).view_model.instances().is_empty());
    });
}
