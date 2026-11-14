use super::*;
use gpui::{TestAppContext, VisualTestContext};

fn compute_with_instances(
    id: &str,
    instances: Vec<clouddeck_core::EC2Instance>,
) -> std::sync::Arc<dyn Provider> {
    std::sync::Arc::new(clouddeck_core::ComputeServiceProvider::new(
        id,
        vec![clouddeck_core::Connection {
            id: "default".into(),
            display_name: "default".to_string(),
            detail: None,
            tags: Vec::new(),
        }],
        vec!["us-east-1".to_string()],
        move |_, _| {
            Box::new(clouddeck_core::MockComputeService::new(
                instances.clone(),
                None,
            ))
        },
    ))
}

fn failing_compute_provider(id: &str) -> std::sync::Arc<dyn Provider> {
    std::sync::Arc::new(clouddeck_core::ComputeServiceProvider::new(
        id,
        vec![clouddeck_core::Connection {
            id: "default".into(),
            display_name: "default".to_string(),
            detail: None,
            tags: Vec::new(),
        }],
        vec!["us-east-1".to_string()],
        |_, _| {
            Box::new(clouddeck_core::MockComputeService::new(
                Vec::new(),
                Some("upstream unavailable".into()),
            ))
        },
    ))
}

fn build_shell(
    cx: &mut TestAppContext,
    settings: Settings,
) -> (Entity<AppShell>, &mut VisualTestContext) {
    add_rooted_shell(cx, move |window, cx| {
        AppShell::build_with_profiles(
            window,
            false,
            DemoFleet::Happy,
            Ok(Vec::new()),
            None,
            settings,
            cx,
        )
    })
}

fn wait_until_loaded(cx: &mut VisualTestContext, shell: &Entity<AppShell>) {
    for _ in 0..200 {
        cx.run_until_parked();
        let loading = shell.read_with(cx, |shell, cx| shell.fleet.read(cx).view_model.is_loading());
        if !loading {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("providers did not finish loading within the timeout");
}

fn all_providers_fleet(
    shell: &Entity<AppShell>,
    cx: &mut VisualTestContext,
) -> Entity<crate::fleet_state::FleetState> {
    shell.read_with(cx, |shell, _| shell.fleet.clone())
}

#[gpui::test]
fn persisted_all_providers_loads_every_signed_in_provider(cx: &mut TestAppContext) {
    install_providers(
        cx,
        vec![
            compute_with_instances("aws", vec![bare_ec2("us-east-1", "i-1")]),
            compute_with_instances("gcp", vec![bare_ec2("us-east-1", "i-2")]),
        ],
    );
    let settings = Settings {
        active_provider: Some(ALL_PROVIDERS.to_string()),
        ..Settings::default()
    };
    let (shell, cx) = build_shell(cx, settings);
    wait_until_loaded(cx, &shell);

    shell.read_with(cx, |shell, _| {
        assert!(shell.context_model.shows_all_providers());
    });
    let fleet = all_providers_fleet(&shell, cx);
    fleet.read_with(cx, |fleet, _| {
        assert!(fleet.view_model.is_all());
        let names: Vec<String> = fleet.view_model.provider_names().into_keys().collect();
        assert_eq!(names, ["aws", "gcp"]);
        assert!(fleet.view_model.provider_failures().is_empty());
        let rows: Vec<String> = fleet
            .view_model
            .rows()
            .into_iter()
            .map(|(provider, _)| provider)
            .collect();
        assert_eq!(rows, ["aws", "gcp"]);
    });
}

#[gpui::test]
fn failing_provider_is_a_failed_group_and_the_others_still_show(cx: &mut TestAppContext) {
    install_providers(
        cx,
        vec![
            compute_with_instances("aws", vec![bare_ec2("us-east-1", "i-1")]),
            failing_compute_provider("gcp"),
        ],
    );
    let settings = Settings {
        active_provider: Some(ALL_PROVIDERS.to_string()),
        ..Settings::default()
    };
    let (shell, cx) = build_shell(cx, settings);
    wait_until_loaded(cx, &shell);

    let fleet = all_providers_fleet(&shell, cx);
    fleet.read_with(cx, |fleet, _| {
        let failures = fleet.view_model.provider_failures();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].provider_id, "gcp");
        assert!(!failures[0].needs_sign_in);
        let rows: Vec<String> = fleet
            .view_model
            .rows()
            .into_iter()
            .map(|(provider, _)| provider)
            .collect();
        assert_eq!(rows, ["aws"]);
    });
}

#[gpui::test]
fn signed_out_provider_is_listed_as_needing_sign_in(cx: &mut TestAppContext) {
    install_providers(
        cx,
        vec![
            compute_provider("aws"),
            std::sync::Arc::new(Unauthenticated(compute_provider("gcp"))),
        ],
    );
    let settings = Settings {
        active_provider: Some(ALL_PROVIDERS.to_string()),
        ..Settings::default()
    };
    let (shell, cx) = build_shell(cx, settings);
    wait_until_loaded(cx, &shell);

    let fleet = all_providers_fleet(&shell, cx);
    fleet.read_with(cx, |fleet, _| {
        let failures = fleet.view_model.provider_failures();
        let gcp = failures
            .iter()
            .find(|failure| failure.provider_id == "gcp")
            .expect("gcp is listed");
        assert!(gcp.needs_sign_in);
        assert!(failures.iter().all(|failure| failure.provider_id != "aws"));
    });
}

#[gpui::test]
fn switching_all_then_aws_then_all_keeps_the_choice(cx: &mut TestAppContext) {
    install_registry(cx, compute_provider("gcp"));
    let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);
    let settings_file =
        std::env::temp_dir().join(format!("clouddeck-all-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&settings_file);

    shell.update_in(cx, |shell, window, cx| {
        shell.switch_provider_to(ALL_PROVIDERS, &settings_file, window, cx)
    });
    cx.run_until_parked();
    let fleet = all_providers_fleet(&shell, cx);
    assert!(fleet.read_with(cx, |fleet, _| fleet.view_model.is_all()));

    shell.update_in(cx, |shell, window, cx| {
        shell.switch_provider_to("aws", &settings_file, window, cx)
    });
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.context_model.active_provider(), "aws");
    });
    let fleet = all_providers_fleet(&shell, cx);
    assert!(!fleet.read_with(cx, |fleet, _| fleet.view_model.is_all()));

    shell.update_in(cx, |shell, window, cx| {
        shell.switch_provider_to(ALL_PROVIDERS, &settings_file, window, cx)
    });
    cx.run_until_parked();
    assert_eq!(
        Settings::load(&settings_file).active_provider.as_deref(),
        Some(ALL_PROVIDERS)
    );
    let fleet = all_providers_fleet(&shell, cx);
    assert!(fleet.read_with(cx, |fleet, _| fleet.view_model.is_all()));
    let _ = std::fs::remove_file(&settings_file);
}

#[gpui::test]
fn unregistered_provider_id_leaves_the_active_provider_unchanged(cx: &mut TestAppContext) {
    let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);
    let before = shell.read_with(cx, |shell, _| {
        shell.context_model.active_provider().to_string()
    });
    let main_window = shell.read_with(cx, |shell, _| shell.main_window.clone());

    main_window.update(cx, |_, cx| {
        cx.emit(MainWindowEvent::PickProvider(
            "no-such-provider".to_string(),
        ))
    });
    cx.run_until_parked();

    let after = shell.read_with(cx, |shell, _| {
        shell.context_model.active_provider().to_string()
    });
    assert_eq!(before, after);
}
