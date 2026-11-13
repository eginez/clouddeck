use super::*;
use gpui::{TestAppContext, VisualTestContext};

fn scoped_provider_id(shell: &Entity<AppShell>, cx: &mut VisualTestContext) -> String {
    shell.read_with(cx, |shell, cx| {
        shell
            .fleet
            .read(cx)
            .view_model
            .provider_names()
            .into_keys()
            .next()
            .expect("one provider is listed")
    })
}

#[gpui::test]
fn switching_provider_persists_the_choice_and_rebuilds_the_fleet(cx: &mut TestAppContext) {
    install_registry(cx, compute_provider("gcp"));
    let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);
    let settings_file =
        std::env::temp_dir().join(format!("clouddeck-switch-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&settings_file);
    let fleet_before = shell.read_with(cx, |shell, _| shell.fleet.entity_id());

    shell.update_in(cx, |shell, window, cx| {
        shell.switch_provider_to("gcp", &settings_file, window, cx)
    });
    cx.run_until_parked();

    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.context_model.active_provider(), "gcp");
        assert_eq!(shell.context_model.connections().len(), 1);
    });
    assert_eq!(
        Settings::load(&settings_file).active_provider.as_deref(),
        Some("gcp")
    );
    let fleet_after = shell.read_with(cx, |shell, _| shell.fleet.entity_id());
    assert_ne!(fleet_before, fleet_after);

    shell.update_in(cx, |shell, window, cx| {
        shell.switch_provider_to("aws", &settings_file, window, cx)
    });
    cx.run_until_parked();

    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.context_model.active_provider(), "aws");
    });
    assert_eq!(
        Settings::load(&settings_file).active_provider.as_deref(),
        Some("aws")
    );
    let _ = std::fs::remove_file(&settings_file);
}

#[gpui::test]
fn switching_to_a_provider_that_needs_sign_in_shows_the_prompt(cx: &mut TestAppContext) {
    install_registry(cx, std::sync::Arc::new(SignedOut(compute_provider("gcp"))));
    let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);
    let settings_file =
        std::env::temp_dir().join(format!("clouddeck-sign-in-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&settings_file);

    shell.update_in(cx, |shell, window, cx| {
        shell.switch_provider_to("gcp", &settings_file, window, cx)
    });
    cx.run_until_parked();

    shell.read_with(cx, |shell, _| {
        assert!(shell.context_model.needs_sign_in());
    });
    let main_window = shell.read_with(cx, |shell, _| shell.main_window.clone());
    assert!(main_window.read_with(cx, |main_window, _| main_window.sign_in));
    let _ = std::fs::remove_file(&settings_file);
}

#[gpui::test]
fn persisted_active_provider_is_used_at_launch(cx: &mut TestAppContext) {
    install_registry(cx, compute_provider("gcp"));
    let settings = Settings {
        active_provider: Some("gcp".to_string()),
        ..Settings::default()
    };
    let (shell, cx) = add_rooted_shell(cx, move |window, cx| {
        AppShell::build_with_profiles(
            window,
            false,
            DemoFleet::Happy,
            Ok(Vec::new()),
            None,
            settings,
            cx,
        )
    });

    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.context_model.active_provider(), "gcp");
    });
    assert_eq!(scoped_provider_id(&shell, cx), "gcp");
}

#[gpui::test]
fn sign_in_event_for_an_unknown_provider_flashes_and_opens_nothing(cx: &mut TestAppContext) {
    let (shell, cx) = add_rooted_shell(cx, AppShell::new_demo);

    emit_main_window_event(cx, &shell, MainWindowEvent::SignIn("nope".to_string()));

    shell.read_with(cx, |shell, cx| {
        assert_eq!(
            shell.fleet.read(cx).flash.as_deref(),
            Some("nope is not installed")
        );
    });
}

#[cfg(test)]
mod changes_aws_profiles_tests {
    use super::*;

    #[test]
    fn only_aws_profile_folder_settings_reload_the_profiles() {
        assert!(changes_aws_profiles("aws", Some("config_dir")));
        assert!(changes_aws_profiles("aws", Some("config_file")));
        assert!(changes_aws_profiles("aws", None));
        assert!(!changes_aws_profiles("aws", Some("request_timeout")));
        assert!(!changes_aws_profiles("gcp", Some("config_dir")));
    }
}
