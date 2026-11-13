use super::*;
use gpui::TestAppContext;

#[gpui::test]
fn saved_ui_state_is_restored_on_startup(cx: &mut TestAppContext) {
    let path = temp_ui_state_path("restore");
    UiState {
        dimension_selections: BTreeMap::from([(
            REGION.to_string(),
            vec!["us-east-1".into(), "eu-west-1".into()],
        )]),
        pinned_searches: vec!["state:stopped".into()],
        recent_searches: vec!["owner:esteban".into()],
        recent_instances: vec!["us-east-1:i-1".into()],
        sidebar_open: false,
        inspector_open: false,
        ..UiState::default()
    }
    .save(&path)
    .unwrap();

    let build_path = path.clone();
    let (shell, cx) = add_rooted_shell(cx, move |window, cx| {
        AppShell::build(window, true, DemoFleet::Partial, Some(build_path), cx)
    });

    shell.read_with(cx, |shell, cx| {
        let fleet = shell.fleet.read(cx);
        assert_eq!(fleet.pinned_searches, vec!["state:stopped".to_string()]);
        assert_eq!(fleet.recent_searches, vec!["owner:esteban".to_string()]);
        assert_eq!(fleet.recent_instances, vec!["us-east-1:i-1".to_string()]);
        assert!(!fleet.sidebar_open);
        assert!(!fleet.inspector_open);
        assert_eq!(
            region_values(&shell.context_model),
            ["us-east-1".to_string(), "eu-west-1".to_string()]
        );
        assert_eq!(
            region_values(&shell.main_window.read(cx).context_model),
            ["us-east-1".to_string(), "eu-west-1".to_string()]
        );
    });
    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[gpui::test]
fn ui_state_saves_once_the_layout_settles(cx: &mut TestAppContext) {
    let path = temp_ui_state_path("debounce");
    let build_path = path.clone();
    let (shell, cx) = add_rooted_shell(cx, move |window, cx| {
        AppShell::build(window, true, DemoFleet::Happy, Some(build_path), cx)
    });
    wait_until(cx, "the startup load to settle", |cx| {
        shell.read_with(cx, |shell, cx| {
            !shell.fleet.read(cx).view_model.is_loading()
        })
    });

    let sidebar_before = shell.read_with(cx, |shell, cx| shell.fleet.read(cx).sidebar_open);
    shell.update(cx, |shell, cx| {
        shell.fleet.update(cx, |fleet, cx| {
            fleet.pinned_searches.push("state:stopped".into());
            fleet.sidebar_open = !fleet.sidebar_open;
            cx.notify();
        });
    });
    cx.executor().advance_clock(Duration::from_millis(900));
    cx.run_until_parked();
    let saved_early = UiState::load(&path);
    assert!(
        !saved_early
            .pinned_searches
            .contains(&"state:stopped".to_string()),
        "the save should wait out the debounce"
    );
    assert!(
        !path.exists(),
        "nothing should be written before the debounce elapses"
    );

    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    let saved = UiState::load(&path);
    assert_eq!(saved.pinned_searches, vec!["state:stopped".to_string()]);
    assert_eq!(saved.sidebar_open, !sidebar_before);
    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

fn install_test_providers(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let registry = clouddeck_core::extension_host::app_registry(
            None,
            std::path::Path::new("/nonexistent-clouddeck-extensions"),
            std::path::Path::new("/nonexistent-clouddeck-state"),
            vec![test_aws_provider()],
        );
        cx.set_global(crate::app_providers::AppProviders::new(registry));
        crate::app_providers::observe_quit(cx);
    });
}

fn provider_shutdowns(cx: &TestAppContext) -> usize {
    cx.read(|cx| {
        cx.global::<crate::app_providers::AppProviders>()
            .shutdowns()
    })
}

#[gpui::test]
fn quit_shuts_the_providers_down_once(cx: &mut TestAppContext) {
    install_test_providers(cx);
    cx.update(|cx| cx.shutdown());
    assert_eq!(provider_shutdowns(cx), 1);
}

#[cfg(target_os = "linux")]
#[gpui::test]
fn closing_the_main_window_shuts_the_providers_down(cx: &mut TestAppContext) {
    install_test_providers(cx);
    let (_shell, cx) = add_rooted_shell(cx, |window, cx| {
        AppShell::build(window, true, DemoFleet::Happy, None, cx)
    });
    assert!(cx.simulate_close());
    assert_eq!(provider_shutdowns(&cx.cx), 1);
}

#[gpui::test]
fn pending_ui_state_is_flushed_on_quit(cx: &mut TestAppContext) {
    let path = temp_ui_state_path("quit-flush");
    let build_path = path.clone();
    let (shell, cx) = add_rooted_shell(cx, move |window, cx| {
        AppShell::build(window, true, DemoFleet::Happy, Some(build_path), cx)
    });
    wait_until(cx, "the startup load to settle", |cx| {
        shell.read_with(cx, |shell, cx| {
            !shell.fleet.read(cx).view_model.is_loading()
        })
    });

    shell.update(cx, |shell, cx| {
        shell.fleet.update(cx, |fleet, cx| {
            fleet.sidebar_open = !fleet.sidebar_open;
            cx.notify();
        });
    });
    let sidebar_open = shell.read_with(cx, |shell, cx| shell.fleet.read(cx).sidebar_open);
    cx.run_until_parked();
    assert!(
        !path.exists(),
        "the change should still be inside the debounce window"
    );

    cx.cx.update(|cx| cx.shutdown());

    assert_eq!(UiState::load(&path).sidebar_open, sidebar_open);
    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[gpui::test]
fn hidden_columns_are_restored_and_saved(cx: &mut TestAppContext) {
    let path = temp_ui_state_path("hidden-columns");
    UiState {
        hidden_columns: vec!["az".into()],
        ..UiState::default()
    }
    .save(&path)
    .unwrap();
    let build_path = path.clone();
    let (shell, cx) = add_rooted_shell(cx, move |window, cx| {
        AppShell::build(window, true, DemoFleet::Happy, Some(build_path), cx)
    });
    assert_eq!(
        shell.read_with(cx, |shell, cx| shell.fleet.read(cx).hidden_columns.clone()),
        [Column::Az]
    );

    shell.update(cx, |shell, cx| {
        shell.fleet.update(cx, |fleet, cx| {
            fleet.toggle_column(Column::Launched, cx);
        });
        shell.save_ui_state(cx);
    });
    assert_eq!(UiState::load(&path).hidden_columns, ["az", "launched"]);
    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[gpui::test]
fn corrupt_ui_state_is_not_overwritten_by_autosave(cx: &mut TestAppContext) {
    let path = temp_ui_state_path("corrupt-autosave");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"not json").unwrap();
    let build_path = path.clone();
    let (shell, cx) = add_rooted_shell(cx, move |window, cx| {
        AppShell::build(window, true, DemoFleet::Happy, Some(build_path), cx)
    });
    wait_until(cx, "the startup load to settle", |cx| {
        shell.read_with(cx, |shell, cx| {
            !shell.fleet.read(cx).view_model.is_loading()
        })
    });

    shell.update(cx, |shell, cx| {
        shell.fleet.update(cx, |fleet, cx| {
            fleet.sidebar_open = !fleet.sidebar_open;
            cx.notify();
        });
    });
    shell.read_with(cx, |shell, _| {
        assert!(
            shell
                .message
                .as_deref()
                .is_some_and(|message| message.contains("UI state")),
            "an unreadable UI state file should be reported in the shell"
        );
    });
    cx.executor().advance_clock(Duration::from_millis(1100));
    cx.run_until_parked();
    assert_eq!(std::fs::read(&path).unwrap(), b"not json");

    cx.cx.update(|cx| cx.shutdown());
    assert_eq!(std::fs::read(&path).unwrap(), b"not json");
    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[gpui::test]
fn settings_clears_are_not_written_back_by_the_next_save(cx: &mut TestAppContext) {
    let path = temp_ui_state_path("settings-clears");
    UiState {
        dimension_selections: BTreeMap::from([(
            REGION.to_string(),
            vec!["us-east-1".into(), "eu-west-1".into()],
        )]),
        recent_searches: vec!["owner:esteban".into()],
        ..UiState::default()
    }
    .save(&path)
    .unwrap();
    let build_path = path.clone();
    let (shell, cx) = add_rooted_shell(cx, move |window, cx| {
        AppShell::build(window, true, DemoFleet::Partial, Some(build_path), cx)
    });

    let mut on_disk = UiState::load(&path);
    on_disk.recent_searches.clear();
    on_disk.dimension_selections.clear();
    on_disk.save(&path).unwrap();
    shell.update(cx, |shell, cx| {
        shell.handle_settings_event(&SettingsEvent::RecentSearchesCleared, cx);
        shell.handle_settings_event(&SettingsEvent::RememberedScopeForgotten, cx);
        shell.fleet.update(cx, |fleet, cx| {
            fleet.sidebar_open = !fleet.sidebar_open;
            cx.notify();
        });
        shell.save_ui_state(cx);
    });

    let saved = UiState::load(&path);
    assert!(saved.recent_searches.is_empty());
    assert!(saved.dimension_selections.is_empty());
    shell.read_with(cx, |shell, cx| {
        assert!(shell.fleet.read(cx).recent_searches.is_empty());
        assert_eq!(
            region_values(&shell.context_model),
            ["us-east-1".to_string(), "eu-west-1".to_string()]
        );
    });

    shell.update(cx, |shell, cx| {
        shell.fleet.update(cx, |fleet, cx| {
            fleet.recent_searches = vec!["owner:esteban".into()];
            cx.notify();
        });
        shell.save_ui_state(cx);
    });
    assert_eq!(
        UiState::load(&path).recent_searches,
        vec!["owner:esteban".to_string()]
    );
    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}
