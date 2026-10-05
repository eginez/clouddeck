use crate::{
    ec2_tab::EC2Tab,
    status_bar::{CloudContextStatusBar, ContextStatusState},
    theme::SurgicalTheme,
};
use clouddeck_core::{
    AwsSdkComputeService, ComputeService, EC2Instance, EC2Scenario, FleetComputeService,
    InstanceListViewModel, MockComputeService, WorkspaceContext,
};
use gpui::{
    Context, Entity, IntoElement, KeyBinding, ParentElement, Render, Styled, Window, actions, div,
    prelude::*, px,
};

actions!(clouddeck, [Refresh]);

pub fn app_key_bindings() -> [KeyBinding; 2] {
    [
        KeyBinding::new("secondary-q", crate::Quit, None),
        KeyBinding::new("secondary-r", Refresh, None),
    ]
}

pub struct AppShell {
    ec2_tab: Entity<EC2Tab>,
    status_bar: Entity<CloudContextStatusBar>,
    context: WorkspaceContext,
    demo: bool,
    demo_scenario: DemoScenario,
    wizard: Option<Entity<crate::account_window::AccountWindowView>>,
    message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DemoScenario {
    Happy,
    Empty,
    Failure,
    Partial,
    Large,
}

fn launch_options() -> (bool, DemoScenario) {
    let mut demo = false;
    let mut scenario = DemoScenario::Happy;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--demo" => demo = true,
            "--demo-scenario" => {
                scenario = match args.next().as_deref() {
                    Some("empty") => DemoScenario::Empty,
                    Some("failure") => DemoScenario::Failure,
                    Some("partial") => DemoScenario::Partial,
                    Some("large") => DemoScenario::Large,
                    _ => DemoScenario::Happy,
                };
                demo = true;
            }
            _ => {}
        }
    }
    (demo, scenario)
}

fn scenario_for(kind: DemoScenario) -> EC2Scenario {
    let fixture = match kind {
        DemoScenario::Empty => {
            include_str!("../../clouddeck-core/testdata/ec2-scenarios/empty-fleet.json")
        }
        _ => include_str!("../../clouddeck-core/testdata/ec2-scenarios/small-happy-path.json"),
    };
    clouddeck_core::parse_scenario(fixture).expect("bundled demo")
}

impl AppShell {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let (demo, scenario) = launch_options();
        Self::build_with_scenario(demo, scenario, cx)
    }

    #[cfg(test)]
    fn new_demo(cx: &mut Context<Self>) -> Self {
        Self::build_with_scenario(true, DemoScenario::Happy, cx)
    }

    fn profiles(
        demo: bool,
        demo_scenario: DemoScenario,
    ) -> Result<Vec<clouddeck_core::AwsProfile>, std::io::Error> {
        if demo {
            Ok(scenario_for(demo_scenario)
                .profiles
                .into_iter()
                .map(|p| clouddeck_core::AwsProfile {
                    name: p.name,
                    region: p.regions.first().map(|r| r.name.clone()),
                })
                .collect())
        } else {
            clouddeck_core::load_profiles()
        }
    }

    fn regions(demo: bool, demo_scenario: DemoScenario) -> Vec<String> {
        if !demo {
            return clouddeck_core::REGIONS
                .iter()
                .map(|region| (*region).into())
                .collect();
        }
        let mut regions = vec!["fleet".to_string()];
        for region in scenario_for(demo_scenario)
            .profiles
            .into_iter()
            .flat_map(|profile| profile.regions.into_iter().map(|region| region.name))
        {
            if !regions.iter().any(|known| known == &region) {
                regions.push(region);
            }
        }
        regions
    }

    fn instances_for(data: &EC2Scenario, profile: &str, region: &str) -> Vec<EC2Instance> {
        data.profiles
            .iter()
            .find(|candidate| candidate.name == profile)
            .map(|candidate| {
                candidate
                    .regions
                    .iter()
                    .filter(|candidate| region == "fleet" || candidate.name == region)
                    .flat_map(|candidate| {
                        candidate.instances.iter().cloned().map(|mut instance| {
                            instance.region = Some(candidate.name.clone());
                            instance
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn large_instances(mut instances: Vec<EC2Instance>) -> Vec<EC2Instance> {
        let seed = instances.clone();
        if seed.is_empty() {
            return instances;
        }
        for index in 4..=120 {
            let mut instance = seed[(index - 4) % seed.len()].clone();
            instance.instance_id = format!("i-demo-{index:03}");
            if let Some(name) = instance.tags.iter_mut().find(|tag| tag.key == "Name") {
                name.value = format!("demo-{index}");
            }
            instances.push(instance);
        }
        instances
    }

    fn demo_service(
        context: &WorkspaceContext,
        demo_scenario: DemoScenario,
    ) -> Box<dyn ComputeService + Send + Sync> {
        let data = scenario_for(demo_scenario);
        if demo_scenario == DemoScenario::Failure {
            return Box::new(MockComputeService::new(
                vec![],
                Some(std::io::Error::other("Simulated demo load failure").into()),
            ));
        }
        if context.region == "fleet" && demo_scenario == DemoScenario::Partial {
            let profile = context.profile.clone();
            return Box::new(FleetComputeService::new(
                clouddeck_core::FLEET_REGIONS
                    .iter()
                    .map(|region| (*region).into())
                    .collect(),
                move |region| {
                    if region == "us-east-1" {
                        Box::new(MockComputeService::new(
                            Self::instances_for(&data, &profile, region),
                            None,
                        ))
                    } else {
                        Box::new(MockComputeService::new(
                            vec![],
                            Some(
                                std::io::Error::other(format!("Simulated failure in {region}"))
                                    .into(),
                            ),
                        ))
                    }
                },
            ));
        }
        let mut instances = Self::instances_for(&data, &context.profile, &context.region);
        if demo_scenario == DemoScenario::Large {
            instances = Self::large_instances(instances);
        }
        Box::new(MockComputeService::new(instances, None))
    }

    fn service(
        context: &WorkspaceContext,
        demo: bool,
        demo_scenario: DemoScenario,
    ) -> Box<dyn ComputeService + Send + Sync> {
        if demo {
            return Self::demo_service(context, demo_scenario);
        }
        if context.profile.is_empty() {
            return Box::new(MockComputeService::new(vec![], Some(std::io::Error::other("Choose an AWS profile using Set up AWS. You can explore the demo without credentials.").into())));
        }
        if context.region == "fleet" {
            let profile = context.profile.clone();
            Box::new(FleetComputeService::new(
                clouddeck_core::FLEET_REGIONS
                    .iter()
                    .map(|r| (*r).into())
                    .collect(),
                move |region| Box::new(AwsSdkComputeService::new(profile.clone(), region)),
            ))
        } else {
            Box::new(AwsSdkComputeService::new(
                context.profile.clone(),
                context.region.clone(),
            ))
        }
    }

    fn build_with_scenario(
        demo: bool,
        demo_scenario: DemoScenario,
        cx: &mut Context<Self>,
    ) -> Self {
        let loaded = Self::profiles(demo, demo_scenario);
        let message = loaded
            .as_ref()
            .err()
            .map(|e| format!("Could not read AWS profiles: {e}"));
        let profiles = loaded.unwrap_or_default();
        let saved = if demo {
            None
        } else {
            WorkspaceContext::load(&clouddeck_core::context_path())
        };
        let context = saved
            .filter(|s| profiles.iter().any(|p| p.name == s.profile))
            .unwrap_or_else(|| WorkspaceContext {
                profile: profiles.first().map(|p| p.name.clone()).unwrap_or_default(),
                region: if demo && demo_scenario == DemoScenario::Partial {
                    "fleet".into()
                } else {
                    profiles
                        .first()
                        .and_then(|p| p.region.clone())
                        .unwrap_or_else(|| "us-east-1".into())
                },
            });
        let view_model = InstanceListViewModel::new(Self::service(&context, demo, demo_scenario));
        let ec2_tab = cx.new(|cx| EC2Tab::new(view_model, cx));
        cx.observe(&ec2_tab, |_, _, cx| cx.notify()).detach();
        // Load asynchronously via `InstanceListView::refresh`'s cx.spawn
        // pattern (background_executor().block() would freeze the UI
        // thread once this wraps a real, network-latency-bearing
        // ComputeService). Demo fixtures resolve almost instantly through
        // the same path, and live/FleetComputeService loads keep the
        // window responsive while they're in flight.
        ec2_tab
            .read(cx)
            .list
            .clone()
            .update(cx, |list, cx| list.refresh(cx));
        let status_bar = cx.new(|_| CloudContextStatusBar {
            provider: if demo {
                "DEMO · AWS".into()
            } else {
                "AWS".into()
            },
            profile: (!context.profile.is_empty()).then(|| context.profile.clone()),
            region: context.region.clone(),
            profiles: Some(
                profiles
                    .into_iter()
                    .map(|p| crate::quick_picker::AwsProfile {
                        name: p.name,
                        hint: p.region.unwrap_or_default(),
                    })
                    .collect(),
            ),
            regions: Some(Self::regions(demo, demo_scenario)),
            ..Default::default()
        });
        cx.observe(&status_bar, |this: &mut Self, bar, cx| {
            if bar.read(cx).demo_requested {
                let demo = !this.demo;
                let scenario = this.demo_scenario;
                bar.update(cx, |bar, _| bar.demo_requested = false);
                *this = Self::build_with_scenario(demo, scenario, cx);
                cx.notify();
                return;
            }
            let next = WorkspaceContext {
                profile: bar.read(cx).profile.clone().unwrap_or_default(),
                region: bar.read(cx).region.clone(),
            };
            if bar.read(cx).manage_requested {
                bar.update(cx, |bar, _| bar.manage_requested = false);
                this.open_setup(cx);
            }
            if next != this.context {
                this.apply_context(next, cx);
            }
        })
        .detach();
        Self {
            ec2_tab,
            status_bar,
            context,
            demo,
            demo_scenario,
            message,
            wizard: None,
        }
    }

    fn apply_context(&mut self, context: WorkspaceContext, cx: &mut Context<Self>) {
        self.context = context;
        self.status_bar.update(cx, |bar, _| {
            bar.profile = (!self.context.profile.is_empty()).then(|| self.context.profile.clone());
            bar.region = self.context.region.clone();
        });
        let vm =
            InstanceListViewModel::new(Self::service(&self.context, self.demo, self.demo_scenario));
        self.ec2_tab = cx.new(|cx| EC2Tab::new(vm, cx));
        cx.observe(&self.ec2_tab, |_, _, cx| cx.notify()).detach();
        self.ec2_tab
            .read(cx)
            .list
            .clone()
            .update(cx, |list, cx| list.refresh(cx));
        if !self.demo {
            self.message = self
                .context
                .save(&clouddeck_core::context_path())
                .err()
                .map(|e| format!("Could not remember this context: {e}"));
        }
        cx.notify();
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.ec2_tab
            .read(cx)
            .list
            .clone()
            .update(cx, |list, cx| list.refresh(cx));
    }

    fn open_setup(&mut self, cx: &mut Context<Self>) {
        use crate::account_window::{AccountWindowMode, AccountWindowView};
        let profiles = match Self::profiles(self.demo, self.demo_scenario) {
            Ok(profiles) => profiles,
            Err(error) => {
                self.message = Some(format!("Could not read AWS profiles: {error}"));
                vec![]
            }
        };
        let picker_profiles = profiles
            .iter()
            .cloned()
            .map(|profile| crate::quick_picker::AwsProfile {
                name: profile.name,
                hint: profile.region.unwrap_or_default(),
            })
            .collect();
        // Re-scan every time setup opens so an `aws configure`/`aws sso login`
        // performed while CloudDeck is running is reflected immediately.
        self.status_bar.update(cx, |bar, cx| {
            bar.profiles = Some(picker_profiles);
            cx.notify();
        });
        let profile = self.context.profile.clone();
        let region = self.context.region.clone();
        let mode = if profile.is_empty() {
            AccountWindowMode::SetupRequired
        } else {
            AccountWindowMode::ManageContext
        };
        let demo = self.demo;
        let demo_scenario = self.demo_scenario;
        let bar = self.status_bar.clone();
        let bounds = gpui::Bounds::centered(None, gpui::size(px(1000.0), px(760.0)), cx);
        let result = cx.open_window(
            gpui::WindowOptions {
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("CloudDeck Setup".into()),
                    ..Default::default()
                }),
                window_bounds: Some(gpui::WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            move |_, cx| {
                let wizard = cx.new(|_| AccountWindowView {
                    profiles: Some(profiles),
                    available_regions: Some(Self::regions(demo, demo_scenario)),
                    selected_profile: if profile.is_empty() {
                        None
                    } else {
                        Some(profile)
                    },
                    selected_region: region,
                    mode,
                    ..Default::default()
                });
                cx.observe(&wizard, move |wizard, cx| {
                    // "Explore demo fleet" is handled here, not by
                    // `account_connect::drive`: it doesn't touch AWS at all,
                    // it just needs to flip the whole shell into demo mode,
                    // which requires the AppShell entity `drive` doesn't have
                    // access to. Proxy it into the status bar's own
                    // `demo_requested` flag, which the AppShell-level
                    // observer set up in `build_with_scenario` already knows
                    // how to act on.
                    if wizard.read(cx).demo_requested {
                        wizard.update(cx, |view, _cx| view.demo_requested = false);
                        bar.update(cx, |bar, cx| {
                            bar.demo_requested = true;
                            cx.notify();
                        });
                        return;
                    }
                    // Drives the SDK-backed connection check (STS identity,
                    // EC2 access, browser SSO sign-in, rescan) behind the
                    // wizard's one-shot request flags. See
                    // `account_connect` for why this lives in its own
                    // module rather than inline here.
                    crate::account_connect::drive(&wizard, cx);
                    if wizard.read(cx).finished {
                        let profile = wizard.read(cx).selected_profile.clone();
                        let region = wizard.read(cx).selected_region.clone();
                        bar.update(cx, |bar, cx| {
                            bar.profile = profile;
                            bar.region = region;
                            cx.notify();
                        });
                    }
                })
                .detach();
                wizard
            },
        );
        match result {
            Ok(handle) => {
                self.wizard = handle.update(cx, |_, _, cx| cx.entity()).ok();
            }
            Err(error) => {
                self.message = Some(format!("Could not open setup: {error}"));
            }
        }
    }
}

impl Render for AppShell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let vm = &self.ec2_tab.read(cx).list.read(cx).view_model;
        let status = if self.context.profile.is_empty() {
            ContextStatusState::SetupRequired
        } else if vm.is_loading() {
            ContextStatusState::Reloading
        } else if vm.error_message().is_some() {
            ContextStatusState::Error("Load failed · see fleet message".into())
        } else if !vm.partial_failures().is_empty() {
            ContextStatusState::Error("Partial fleet · some regions failed".into())
        } else {
            ContextStatusState::Ready
        };
        self.status_bar.update(cx, |bar, _| bar.status = status);
        div()
            .debug_selector(|| "app-shell".into())
            .flex()
            .flex_col()
            .size_full()
            .bg(SurgicalTheme::workspace_canvas())
            .children(self.message.clone().map(|m| {
                div()
                    .px(px(16.0))
                    .text_color(SurgicalTheme::warning())
                    .child(m)
            }))
            .child(
                div()
                    .debug_selector(|| "shell-main-content".into())
                    .flex_1()
                    .min_h(px(0.0))
                    .min_w(px(0.0))
                    .h_full()
                    .child(self.ec2_tab.clone()),
            )
            .on_action(cx.listener(|this, _: &Refresh, _, cx| this.refresh(cx)))
            .child(
                div()
                    .debug_selector(|| "shell-status-bar".into())
                    .flex_shrink_0()
                    .w_full()
                    .h(px(48.0))
                    .child(self.status_bar.clone()),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::GatedComputeService;
    use gpui::TestAppContext;

    #[gpui::test]
    fn mounts_without_a_display(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, cx| AppShell::new_demo(cx));

        assert!(
            cx.debug_bounds("app-shell").is_some(),
            "app-shell root should render"
        );
    }

    #[gpui::test]
    fn renders_table_and_status_regions_with_real_child_components(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, cx| AppShell::new_demo(cx));

        assert!(
            cx.debug_bounds("shell-main-content").is_some(),
            "main content region should render"
        );
        assert!(
            cx.debug_bounds("shell-sidebar").is_none(),
            "the fleet table should use the full shell width"
        );
        assert!(
            cx.debug_bounds("shell-status-bar").is_some(),
            "status bar region should render"
        );

        assert!(
            cx.debug_bounds("ec2.tab.list-only")
                .or(cx.debug_bounds("ec2.tab.split"))
                .is_some(),
            "the real EC2Tab should be mounted inside shell-main-content"
        );
        assert!(
            cx.debug_bounds("statusbar.root").is_some(),
            "the real CloudContextStatusBar should be mounted inside shell-status-bar"
        );
    }

    #[gpui::test]
    fn ec2_tab_shows_the_seeded_sample_instances(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, cx| AppShell::new_demo(cx));
        // The demo scenario now loads via the same cx.spawn path that
        // crosses onto the shared tokio runtime (a real OS thread pool
        // outside gpui's deterministic test executor), so a single
        // run_until_parked() can return before the result lands -- poll
        // with bounded, short real sleeps like
        // startup_load_is_async_and_transitions_from_loading_to_populated
        // does below.
        let mut loaded = false;
        for _ in 0..200 {
            cx.run_until_parked();
            if cx
                .debug_bounds("ec2.instance.row.us-east-1:i-scenario-001")
                .is_some()
            {
                loaded = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(loaded, "the seeded row did not render within the timeout");

        assert!(
            cx.debug_bounds("ec2.tab.list-only").is_some(),
            "no instance is selected initially, so the list-only layout should render"
        );
        assert!(
            cx.debug_bounds("ec2.instance.row.us-east-1:i-scenario-001")
                .is_some(),
            "a seeded sample instance row should render"
        );
    }

    #[gpui::test]
    fn startup_load_is_async_and_transitions_from_loading_to_populated(cx: &mut TestAppContext) {
        // Mirrors the sequence `build_with_scenario` now uses at startup
        // (and `apply_context` on a context switch): build the `EC2Tab`
        // entity, then call `list.refresh(cx)` -- the same cx.spawn path
        // the refresh button uses -- instead of blocking the UI thread on
        // `cx.background_executor().block(view_model.load_instances())`.
        let gate = std::sync::Arc::new(tokio::sync::Notify::new());
        let service = GatedComputeService {
            gate: gate.clone(),
            instances: vec![EC2Instance {
                instance_id: "i-gated".to_string(),
                region: Some("us-east-1".to_string()),
                instance_type: None,
                state: None,
                launch_time: None,
                public_ip_address: None,
                private_ip_address: None,
                vpc_id: None,
                subnet_id: None,
                security_groups: vec![],
                tags: vec![],
            }],
        };
        let view_model = InstanceListViewModel::new(Box::new(service));

        let (ec2_tab, cx) = cx.add_window_view(|_, cx| {
            let tab = EC2Tab::new(view_model, cx);
            tab.list.clone().update(cx, |list, cx| list.refresh(cx));
            tab
        });

        assert!(
            cx.debug_bounds("ec2.instance.row.us-east-1:i-gated")
                .is_none(),
            "data should not be populated synchronously while the gated load is in flight"
        );
        assert!(
            ec2_tab.read_with(cx, |tab, cx| tab.list.read(cx).view_model.is_loading()),
            "the view model should report loading while the spawn is in flight"
        );

        gate.notify_one();
        // `EC2Tab`/`InstanceListView::refresh`'s load resolves on the
        // shared tokio runtime, a real OS thread pool outside gpui's
        // deterministic test executor -- a single `run_until_parked()` can
        // return before that cross-thread result lands, so poll with
        // bounded, short real sleeps.
        let mut loaded = false;
        for _ in 0..200 {
            cx.run_until_parked();
            if cx
                .debug_bounds("ec2.instance.row.us-east-1:i-gated")
                .is_some()
            {
                loaded = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(loaded, "the load did not complete within the timeout");

        assert!(
            cx.debug_bounds("ec2.instance.row.us-east-1:i-gated")
                .is_some(),
            "the row should render once the async load completes"
        );
        assert!(
            !ec2_tab.read_with(cx, |tab, cx| tab.list.read(cx).view_model.is_loading()),
            "loading should clear once the spawn resolves"
        );
    }

    #[gpui::test]
    fn table_and_status_are_docked_without_a_sidebar(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, cx| AppShell::new_demo(cx));

        let main_content = cx
            .debug_bounds("shell-main-content")
            .expect("main content should render");
        let status_bar = cx
            .debug_bounds("shell-status-bar")
            .expect("status bar should render");

        assert_eq!(main_content.origin.x, px(0.0));
        assert!(
            status_bar.origin.y >= main_content.origin.y + main_content.size.height,
            "status bar should be docked below the fleet table"
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
    fn demo_regions_only_include_fixture_regions_and_fleet() {
        assert_eq!(
            AppShell::regions(true, DemoScenario::Happy),
            vec!["fleet".to_string(), "us-east-1".to_string()]
        );
        assert!(
            AppShell::regions(true, DemoScenario::Happy)
                .iter()
                .all(|region| region == "fleet" || region == "us-east-1")
        );
    }

    #[test]
    fn demo_failure_and_partial_modes_are_observable_without_aws() {
        let context = WorkspaceContext {
            profile: "scenario-default".into(),
            region: "fleet".into(),
        };

        let failure = AppShell::demo_service(&context, DemoScenario::Failure);
        let failure = crate::runtime::runtime()
            .block_on(failure.list_instances(None))
            .expect_err("failure scenario should fail visibly");
        assert!(failure.to_string().contains("Simulated demo load failure"));

        let partial = AppShell::demo_service(&context, DemoScenario::Partial);
        let partial = crate::runtime::runtime()
            .block_on(partial.list_instances(None))
            .expect("partial scenario should retain successful regions");
        assert_eq!(partial.instances.len(), 3);
        assert_eq!(partial.partial_failures.len(), 3);
    }

    #[test]
    fn large_demo_mode_has_enough_rows_to_exercise_scrolling() {
        let context = WorkspaceContext {
            profile: "scenario-default".into(),
            region: "us-east-1".into(),
        };
        let service = AppShell::demo_service(&context, DemoScenario::Large);
        let result = crate::runtime::runtime()
            .block_on(service.list_instances(None))
            .expect("large demo should load");
        assert_eq!(result.instances.len(), 120);
    }

    #[gpui::test]
    fn opening_setup_creates_the_wizard_and_finishing_it_updates_the_status_bar(
        cx: &mut TestAppContext,
    ) {
        use crate::account_window::AccountConnectionState;

        let (shell, cx) = cx.add_window_view(|_, cx| AppShell::new_demo(cx));

        // The trigger: nothing opens the account-connect window on its own
        // (per clouddeck-rs-w6s.18's design, existing profiles need minimal
        // interaction), but `open_setup` -- reached from the status bar's
        // "Set up AWS"/"Manage AWS" chip and the quick picker's "Manage
        // Accounts..." row -- must actually create a wizard window.
        shell.update(cx, |shell, cx| {
            assert!(shell.wizard.is_none(), "no wizard until setup is opened");
            shell.open_setup(cx);
            assert!(
                shell.wizard.is_some(),
                "open_setup should create the Connect to AWS wizard entity"
            );
        });

        let wizard = shell
            .read_with(cx, |shell, _| shell.wizard.clone())
            .expect("wizard entity");

        // Drive the wizard to a finished connection the same way a real
        // connect (account_connect::drive) would, then confirm the shell's
        // observer flows the result back into the live status bar chips --
        // not `quick_picker::mock_profiles()`/`mock_regions()`.
        wizard.update(cx, |wizard, cx| {
            wizard.selected_profile = Some("scenario-default".into());
            wizard.selected_region = "us-west-2".into();
            wizard.connection_state = AccountConnectionState::Connected {
                account_id: "123456789012".into(),
            };
            wizard.open_fleet();
            cx.notify();
        });

        shell.read_with(cx, |shell, cx| {
            assert_eq!(
                shell.status_bar.read(cx).profile.as_deref(),
                Some("scenario-default"),
                "finishing the wizard should flow its selected profile into the status bar"
            );
            assert_eq!(shell.status_bar.read(cx).region, "us-west-2");
        });
    }

    // Regression test for a bug where the wizard's "Explore demo fleet"
    // action (`demo_requested`) was read and cleared by
    // `account_connect::drive` -- which never acts on it -- every render
    // cycle, so the button was a silent no-op. The wizard's observer must
    // intercept `demo_requested` itself and proxy it into the status bar's
    // own `demo_requested` flag, which `build_with_scenario`'s
    // shell-level observer already knows how to turn into a demo-mode
    // rebuild.
    #[gpui::test]
    fn wizard_demo_request_proxies_into_the_status_bars_demo_toggle(cx: &mut TestAppContext) {
        let (shell, cx) = cx.add_window_view(|_, cx| AppShell::new_demo(cx));

        shell.update(cx, |shell, cx| shell.open_setup(cx));

        let wizard = shell
            .read_with(cx, |shell, _| shell.wizard.clone())
            .expect("wizard entity");
        let demo_before = shell.read_with(cx, |shell, _| shell.demo);

        wizard.update(cx, |wizard, cx| {
            wizard.request_demo();
            cx.notify();
        });

        shell.read_with(cx, |shell, _| {
            assert_ne!(
                shell.demo, demo_before,
                "Explore demo fleet must flip the shell's demo mode, not silently no-op"
            );
        });
        wizard.read_with(cx, |wizard, _| {
            assert!(
                !wizard.demo_requested,
                "the one-shot demo request must be consumed, not left set"
            );
        });
    }

    #[gpui::test]
    fn applying_a_new_demo_context_updates_context_and_clears_old_rows(cx: &mut TestAppContext) {
        let (shell, cx) = cx.add_window_view(|_, cx| AppShell::new_demo(cx));
        shell.update(cx, |shell, cx| {
            shell.apply_context(
                WorkspaceContext {
                    profile: "scenario-default".into(),
                    region: "us-west-2".into(),
                },
                cx,
            );
        });

        shell.read_with(cx, |shell, cx| {
            assert_eq!(shell.context.region, "us-west-2");
            assert_eq!(shell.status_bar.read(cx).region, "us-west-2");
            assert!(
                shell
                    .ec2_tab
                    .read(cx)
                    .list
                    .read(cx)
                    .view_model
                    .instances()
                    .is_empty()
            );
        });
    }
}
