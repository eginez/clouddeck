use super::*;
use gpui::TestAppContext;

fn fresh_context_file() -> PathBuf {
    let path = TEST_CONTEXT_PATH.with(PathBuf::clone);
    let _ = std::fs::remove_file(&path);
    path
}

fn open_onboarding_view(
    cx: &mut gpui::VisualTestContext,
) -> gpui::Entity<crate::onboarding_window::OnboardingWindow> {
    cx.update(|_, app| {
        app.windows().into_iter().find_map(|window| {
            let root = window.downcast::<gpui_component::Root>()?;
            root.read(app)
                .ok()?
                .view()
                .clone()
                .downcast::<crate::onboarding_window::OnboardingWindow>()
                .ok()
        })
    })
    .expect("an onboarding window is open")
}

fn sign_in_and_finish(cx: &mut gpui::VisualTestContext, shell: &Entity<AppShell>, id: &str) {
    shell.update(cx, |shell, cx| shell.sign_in(id, cx));
    let onboarding = open_onboarding_view(cx);
    onboarding.update(cx, |_, cx| {
        cx.emit(OnboardingEvent::SignedIn {
            provider: id.to_string(),
        })
    });
    cx.run_until_parked();
}

#[gpui::test]
fn explore_demo_from_onboarding_turns_demo_mode_on(cx: &mut TestAppContext) {
    let context_file = fresh_context_file();
    install_registry(cx, crate::test_support::auth_provider("gcp", &["default"]));
    let (shell, cx) = add_rooted_shell(cx, |window, cx| {
        AppShell::build_with_profiles(
            window,
            false,
            DemoFleet::Happy,
            Ok(Vec::new()),
            None,
            Settings::default(),
            cx,
        )
    });
    shell.read_with(cx, |shell, _| assert!(!shell.demo));

    shell.update(cx, |shell, cx| shell.sign_in("gcp", cx));
    let onboarding = open_onboarding_view(cx);
    onboarding.update(cx, |_, cx| cx.emit(OnboardingEvent::ExploreDemo));
    cx.run_until_parked();

    shell.read_with(cx, |shell, _| assert!(shell.demo));
    let _ = std::fs::remove_file(&context_file);
}

#[gpui::test]
fn signing_in_loads_connections_and_keeps_the_current_one(cx: &mut TestAppContext) {
    let context_file = fresh_context_file();
    install_registry(
        cx,
        crate::test_support::auth_provider("gcp", &["default", "team"]),
    );
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
    shell.update_in(cx, |shell, window, cx| {
        shell.apply_context(
            WorkspaceContext {
                provider: "gcp".into(),
                profile: String::new(),
            },
            window,
            cx,
        )
    });

    sign_in_and_finish(cx, &shell, "gcp");

    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.context_model.connections().len(), 2);
        assert_eq!(shell.context.profile, "default");
    });

    shell.update_in(cx, |shell, window, cx| {
        shell.apply_context(
            WorkspaceContext {
                provider: "gcp".into(),
                profile: "team".into(),
            },
            window,
            cx,
        )
    });

    sign_in_and_finish(cx, &shell, "gcp");

    shell.read_with(cx, |shell, _| assert_eq!(shell.context.profile, "team"));
    let _ = std::fs::remove_file(&context_file);
}

#[gpui::test]
fn opening_a_connection_from_onboarding_selects_it(cx: &mut TestAppContext) {
    let context_file = fresh_context_file();
    install_registry(
        cx,
        crate::test_support::auth_provider("gcp", &["default", "team"]),
    );
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
    shell.update(cx, |shell, cx| shell.sign_in("gcp", cx));
    let onboarding = open_onboarding_view(cx);
    onboarding.update(cx, |_, cx| {
        cx.emit(OnboardingEvent::Open {
            provider: "gcp".to_string(),
            connection: ConnectionId::from("team"),
        })
    });
    cx.run_until_parked();

    shell.read_with(cx, |shell, _| assert_eq!(shell.context.profile, "team"));
    let _ = std::fs::remove_file(&context_file);
}
