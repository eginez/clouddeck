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
    /// Test-only: an optional compute service injected into `apply_context`
    /// so that a test can deterministically hold a picker-driven context
    /// switch load open (via `GatedComputeService`) and observe the
    /// `is_loading` / `Reloading` transition window that
    /// `MockComputeService` resolves too fast to reliably catch.
    #[cfg(test)]
    test_service_override: Option<
        std::sync::Arc<dyn ComputeService + Send + Sync>,
    >,
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

/// True when the shell is running against real AWS credentials (as opposed
/// to the bundled demo scenario). Public so the fleet view's
/// no-profiles onboarding affordance can decide whether "Connect to AWS"
/// and "Rescan profiles" make sense at all (they do not in demo mode,
/// where the fleet is scenario data). Tests never pass `--demo`, so
/// `cargo test` runs always see real mode here.
pub fn is_real_mode() -> bool {
    !std::env::args_os().any(|arg| arg == "--demo")
}

fn rescan_changed(before: &[String], after: &[clouddeck_core::AwsProfile]) -> bool {
    let after_names: Vec<String> = after.iter().map(|p| p.name.clone()).collect();
    before != after_names.as_slice()
}

/// The adopt-first-profile decision for a post-sign-in rescan, factored out
/// of the shell observer so it can be unit-tested against profile vectors
/// rather than the host's real `~/.aws` (clouddeck-rs-w6s.18).
///
/// Returns the name of the profile to adopt when (and only when) the current
/// context has no profile selected yet *and* the detected set changed since
/// launch (e.g. a browser sign-in created the first profiles). Otherwise
/// `None`: a saved context is never overridden, an unchanged set means
/// nothing new appeared, and an empty set means there is nothing to adopt.
fn profile_to_adopt(
    current_profile: &str,
    before: &[String],
    after: &[clouddeck_core::AwsProfile],
) -> Option<String> {
    if !current_profile.is_empty() || !rescan_changed(before, after) || after.is_empty() {
        return None;
    }
    after.first().map(|p| p.name.clone())
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

    #[cfg(test)]
    fn new_demo_empty(cx: &mut Context<Self>) -> Self {
        Self::build_with_scenario(true, DemoScenario::Empty, cx)
    }

    #[cfg(test)]
    fn new_demo_failure(cx: &mut Context<Self>) -> Self {
        Self::build_with_scenario(true, DemoScenario::Failure, cx)
    }

    #[cfg(test)]
    fn new_demo_large(cx: &mut Context<Self>) -> Self {
        Self::build_with_scenario(true, DemoScenario::Large, cx)
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
            return Box::new(MockComputeService::new(vec![], Some(std::io::Error::other("No AWS profile selected. Use “Connect to AWS” to sign in and choose a profile, or explore the demo without credentials.").into())));
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
        // Remember the profile list feeding the status bar chips at startup,
        // so a later empty-state rescan can tell whether the profile set
        // changed since launch (e.g. a browser sign-in created the first
        // profiles after launch). Shared by value with the rescan observer
        // below.
        let startup_profile_names: std::sync::Arc<Vec<String>> = profiles
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>()
            .into();
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
        let ec2_tab = ec2_tab.clone();
        let observer_startups = startup_profile_names.clone();
        // The fleet's empty state can ask for the setup window ("Connect to
        // AWS" on first launch with no profiles) or for a profile rescan
        // (after a browser sign-in created the first profile) without the
        // shell being rebuilt: consume those one-shot flags here.
        cx.observe(&ec2_tab, move |this: &mut Self, tab, cx| {
            let list = tab.read(cx).list.clone();
            let manage = list.read(cx).manage_requested;
            let rescan = list.read(cx).rescan_requested;
            if manage || rescan {
                list.update(cx, |list, cx| {
                    list.manage_requested = false;
                    list.rescan_requested = false;
                    cx.notify();
                });
                if manage {
                    this.open_setup(cx);
                } else {
                    // Rescan: refresh the profile list feeding the status
                    // bar chips and restart the fleet load against the same
                    // context (a newly created profile may now be usable;
                    // otherwise the fleet errors again, inline and
                    // recoverable).
                    match Self::profiles(this.demo, this.demo_scenario) {
                        Ok(rescanned) => {
                            // Decide the adopt-up-front, before the status bar
                            // update borrows the list, so the rescan-adopt
                            // branch cannot race a move out of `rescanned`
                            // (a previous draft `mem::take`-ed the profiles into
                            // the status bar and then read `.first()`, which
                            // was always `None` — clouddeck-rs-w6s.18 review).
                            // `profile_to_adopt` re-checks the "changed since
                            // launch" condition itself, so no separate `changed`
                            // binding is needed here.
                            let adopted = profile_to_adopt(
                                &this.context.profile,
                                &observer_startups,
                                &rescanned,
                            );
                            this.status_bar.update(cx, |bar, cx| {
                                bar.profiles = Some(
                                    rescanned
                                        .iter()
                                        .map(|p| crate::quick_picker::AwsProfile {
                                            name: p.name.clone(),
                                            hint: p.region.clone().unwrap_or_default(),
                                        })
                                        .collect(),
                                );
                                cx.notify();
                            });
                            if let Some(adopted) = adopted {
                                // No context was ever chosen and the first
                                // profiles just appeared (e.g. a browser
                                // sign-in created them): adopt the first
                                // detected profile instead of restarting the
                                // load against an empty profile name.
                                this.apply_context(
                                    WorkspaceContext {
                                        profile: adopted,
                                        region: "us-east-1".into(),
                                    },
                                    cx,
                                );
                                return;
                            }
                            this.refresh(cx);
                        }
                        Err(error) => {
                            this.message = Some(format!("Could not read AWS profiles: {error}"));
                        }
                    }
                }
            }
        })
        .detach();
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
            #[cfg(test)]
            test_service_override: None,
        }
    }

    /// Test-only: install a compute service that will be used (instead of
    /// the scenario-derived one) by the *next* `apply_context` call, so a
    /// test can deterministically observe the loading window of a
    /// picker-driven context switch. Must be called before the trigger
    /// that causes `apply_context` to fire.
    #[cfg(test)]
    pub fn set_test_service_override(
        &mut self,
        service: std::sync::Arc<dyn ComputeService + Send + Sync>,
    ) {
        self.test_service_override = Some(service);
    }

    fn apply_context(&mut self, context: WorkspaceContext, cx: &mut Context<Self>) {
        self.context = context;
        self.status_bar.update(cx, |bar, _| {
            bar.profile = (!self.context.profile.is_empty()).then(|| self.context.profile.clone());
            bar.region = self.context.region.clone();
        });
        // `refresh` flips the new tab's view model to loading synchronously
        // before its spawned task resolves, so the very next render
        // already reports Reloading from the new tab itself.
        #[cfg(test)]
        let service: Box<dyn ComputeService + Send + Sync> = self
            .test_service_override
            .as_ref()
            .map(|s| {
                // Wrap the shared `Arc<dyn>` in `ArcComputeService` so each
                // new tab gets its own `Box<dyn>` handle over the same
                // underlying service.
                Box::new(crate::test_support::ArcComputeService::new(
                    std::sync::Arc::clone(s),
                )) as _
            })
            .unwrap_or_else(|| {
                Self::service(&self.context, self.demo, self.demo_scenario)
            });
        #[cfg(not(test))]
        let service: Box<dyn ComputeService + Send + Sync> =
            Self::service(&self.context, self.demo, self.demo_scenario);
        let vm = InstanceListViewModel::new(service);
        let new_tab = cx.new(|cx| EC2Tab::new(vm, cx));
        cx.observe(&new_tab, |_, _, cx| cx.notify()).detach();
        new_tab
            .read(cx)
            .list
            .clone()
            .update(cx, |list, cx| list.refresh(cx));
        self.ec2_tab = new_tab;
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
        // The status bar is the app-wide context indicator (it replaced
        // the sidebar's "Active Context" card in clouddeck-rs-wz2.3): a
        // context switch replaces the tab entity in `apply_context` and its
        // `refresh` flips the fresh view model to loading synchronously, so
        // Reloading is visible for the whole transition -- no stale Ready
        // from the previous context's tab.
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
    use clouddeck_core::AwsProfile;
    use gpui::TestAppContext;
    use gpui::VisualTestContext;

    // clouddeck-rs-w6s.18: the adopt-first-profile decision after a rescan,
    // factored into a pure fn so it can be exercised against profile vectors
    // instead of the host's real `~/.aws` (which the shell observer reads).
    fn aws_profile(name: &str, region: Option<&str>) -> AwsProfile {
        AwsProfile {
            name: name.into(),
            region: region.map(Into::into),
        }
    }

    #[test]
    fn profile_to_adopt_returns_first_profile_when_current_is_empty_and_list_changed() {
        let before: Vec<String> = vec![];
        let after = vec![
            aws_profile("new-first", Some("us-east-1")),
            aws_profile("new-second", None),
        ];
        assert_eq!(
            profile_to_adopt("", &before, &after),
            Some("new-first".to_string()),
            "a first launch with no chosen profile and newly appeared profiles adopts the first"
        );
    }

    #[test]
    fn profile_to_adopt_returns_none_when_current_profile_is_non_empty() {
        let before: Vec<String> = vec![];
        let after = vec![aws_profile("new-first", None)];
        assert_eq!(
            profile_to_adopt("existing-profile", &before, &after),
            None,
            "a saved context is never overridden by a rescan"
        );
    }

    #[test]
    fn profile_to_adopt_returns_none_when_profile_list_is_unchanged() {
        let before: Vec<String> = vec!["stable".into()];
        let after = vec![aws_profile("stable", Some("eu-west-1"))];
        assert_eq!(
            profile_to_adopt("", &before, &after),
            None,
            "an unchanged set means nothing new appeared, so nothing is adopted"
        );
    }

    #[test]
    fn profile_to_adopt_returns_none_when_profile_list_is_empty() {
        let before: Vec<String> = vec!["gone".into()];
        let after: Vec<AwsProfile> = vec![];
        assert_eq!(
            profile_to_adopt("", &before, &after),
            None,
            "an empty rescan means there is nothing to adopt"
        );
    }

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

        assert!(
            name.size.width > row.size.width / 2.0,
            "name cell should be at least half the row width (got {name} of {row})"
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

    /// Polls `run_until_parked` + short sleeps until `selector` renders,
    /// mirroring the startup-load pattern the other AppShell tests use
    /// (the demo load crosses onto the shared tokio runtime, so one
    /// `run_until_parked()` can return before the rows land).
    fn wait_for_row(cx: &mut VisualTestContext, selector: &str) {
        for _ in 0..200 {
            cx.run_until_parked();
            let static_selector: &'static str = Box::leak(selector.to_string().into_boxed_str());
            if cx.debug_bounds(static_selector).is_some() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("row {selector} did not render within the timeout");
    }

    #[gpui::test]
    fn instance_row_name_and_id_cells_keep_readable_width_in_appshell_demo(
        cx: &mut TestAppContext,
    ) {
        let (_view, cx) = cx.add_window_view(|_, cx| AppShell::new_demo(cx));
        // The reported regression is at 1280pt window width.
        cx.simulate_resize(gpui::size(px(1280.0), px(800.0)));
        wait_for_row(cx, "ec2.instance.row.us-east-1:i-scenario-001");

        assert_appshell_row_widths(cx, "ec2.instance.row.us-east-1:i-scenario-001");
        assert_appshell_row_widths(cx, "ec2.instance.row.us-east-1:i-scenario-002");
    }

    #[gpui::test]
    fn instance_row_name_and_id_cells_keep_readable_width_in_appshell_large(
        cx: &mut TestAppContext,
    ) {
        let (_view, cx) = cx.add_window_view(|_, cx| AppShell::new_demo_large(cx));
        cx.simulate_resize(gpui::size(px(1280.0), px(800.0)));
        // 121 rows render: the 3 seeded i-scenario rows plus i-demo-004..120
        // (large_instances clones the seeds, so i-demo-120 carries the
        // us-east-1 seed's region).
        wait_for_row(cx, "ec2.instance.row.us-east-1:i-scenario-001");
        wait_for_row(cx, "ec2.instance.row.us-east-1:i-demo-120");

        assert_appshell_row_widths(cx, "ec2.instance.row.us-east-1:i-scenario-001");
        assert_appshell_row_widths(cx, "ec2.instance.row.us-east-1:i-demo-120");
    }

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
                availability_zone: None,
                image_id: None,
                key_name: None,
                architecture: None,
                state_transition_reason: None,
                iam_instance_profile_arn: None,
                private_dns_name: None,
                public_dns_name: None,
            }],
            error: None,
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

    #[gpui::test]
    fn mounting_in_real_mode_without_profiles_shows_setup_affordance(cx: &mut TestAppContext) {
        // A real (non-demo) launch with no detected AWS profiles must reach
        // the fleet with an in-place "Connect to AWS" affordance (the
        // no-profiles onboarding entry point, clouddeck-rs-w6s.18) rather
        // than a dead end. The shell's no-profile branch of `service()`
        // (see `shell.rs`) resolves to a load-failure error panel (not the
        // plain empty panel), so the affordance must render in that error
        // panel too — this test covers exactly that path.
        let real_mode = crate::shell::is_real_mode();
        let (_shell, cx) = cx.add_window_view(|_, cx| {
            if real_mode {
                AppShell::build_with_scenario(false, DemoScenario::Happy, cx)
            } else {
                AppShell::build_with_scenario(true, DemoScenario::Empty, cx)
            }
        });

        // The state (empty or error) renders once the load resolves; the
        // setup affordance must be present by then, so poll for it directly
        // rather than just waiting for any state to appear.
        let mut saw_setup = false;
        for _ in 0..200 {
            cx.run_until_parked();
            if cx.debug_bounds("ec2.instance.empty-setup").is_some() {
                saw_setup = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        assert!(
            cx.debug_bounds("ec2.instance.empty").is_some()
                || cx.debug_bounds("ec2.instance.error").is_some(),
            "the empty/error state should render"
        );
        if real_mode {
            assert!(
                saw_setup,
                "a real-mode launch with no profiles should show Connect to AWS"
            );
        } else {
            assert!(
                cx.debug_bounds("ec2.instance.empty-setup").is_none(),
                "the demo empty scenario must not offer AWS onboarding"
            );
        }
    }

    #[gpui::test]
    fn empty_state_setup_request_opens_the_wizard(cx: &mut TestAppContext) {
        // The empty-fleet "Connect to AWS" affordance sets
        // `manage_requested`; the shell must consume that flag and open the
        // Connect to AWS window (the no-profiles onboarding trigger,
        // clouddeck-rs-w6s.18).
        let (shell, cx) = cx.add_window_view(|_, cx| AppShell::new_demo(cx));

        shell.update(cx, |shell, cx| {
            shell.ec2_tab.read(cx).list.clone().update(cx, |list, cx| {
                list.manage_requested = true;
                cx.notify();
            });
        });
        cx.run_until_parked();

        shell.read_with(cx, |shell, cx| {
            assert!(
                shell.wizard.is_some(),
                "the empty-state setup request should open the Connect to AWS window"
            );
            assert!(
                !shell.ec2_tab.read(cx).list.read(cx).manage_requested,
                "the one-shot setup request must be consumed by the shell"
            );
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

    /// `AppShell::new_demo`'s startup load resolves on the shared tokio
    /// runtime (a real OS thread pool outside gpui's deterministic test
    /// executor), so poll with bounded, short real sleeps rather than
    /// assuming a single `run_until_parked()` lands the result.
    ///
    /// The closure takes a fresh `&mut VisualTestContext` reborrow on each
    /// tick, so it can call `debug_bounds` (a `VisualTestContext`-only
    /// method) or read entity state without holding a conflicting borrow
    /// on `cx` itself for the whole loop.
    fn wait_until(
        cx: &mut gpui::VisualTestContext,
        label: &str,
        mut condition: impl FnMut(&mut gpui::VisualTestContext) -> bool,
    ) {
        for _ in 0..200 {
            cx.run_until_parked();
            if condition(cx) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("timed out waiting for: {label}");
    }

    /// Polls until the switched-in context has finished loading, the status
    /// bar reads `Ready`, and instances are present. Row presence alone is
    /// not enough: the status bar is only refreshed on the next shell render,
    /// so it can lag the view model's settled state.
    fn wait_until_context_settled(
        cx: &mut gpui::VisualTestContext,
        shell: &gpui::Entity<AppShell>,
    ) {
        wait_until(cx, "context load to settle to Ready", |cx| {
            shell.read_with(cx, |shell, cx| {
                let vm = &shell.ec2_tab.read(cx).list.read(cx).view_model;
                !vm.is_loading()
                    && shell.status_bar.read(cx).status == ContextStatusState::Ready
                    && !vm.instances().is_empty()
            })
        });
    }

    /// End-to-end proof that the *real* picker -> shell observer ->
    /// `apply_context` path works, not just `apply_context` called by hand:
    /// clicking the region chip in the live status bar opens the quick
    /// picker, and clicking a row drives the whole shell -- context, status
    /// bar chip, and the EC2Tab's fleet reload -- on a context switch from
    /// Fleet to a concrete region.
    #[gpui::test]
    fn region_picker_row_click_drives_shell_context_and_fleet_reload(cx: &mut TestAppContext) {
        let (shell, cx) = cx.add_window_view(|_, cx| AppShell::new_demo(cx));

        // Let the demo startup load settle so the initial context (the
        // happy-path default: profile scenario-default, region us-east-1)
        // is in its Ready state before we switch.
        wait_until(cx, "startup load to settle", |cx| {
            cx.debug_bounds("ec2.instance.row.us-east-1:i-scenario-001")
                .is_some()
        });

        // Open the region picker from the status bar chip.
        let chip = cx
            .debug_bounds("statusbar.region")
            .expect("region chip should render in the live status bar");
        cx.simulate_click(chip.center(), gpui::Modifiers::default());
        assert!(
            cx.debug_bounds("statusbar.quickpicker").is_some(),
            "the quick picker popover should open from the chip click"
        );

        // Click the Fleet row to switch from the concrete default region
        // to the curated fleet view.
        let row = cx
            .debug_bounds("statusbar.quickpicker.region.row.fleet")
            .expect("the Fleet row should render while the picker is open");
        cx.simulate_click(row.center(), gpui::Modifiers::default());

        // `apply_context` calls `refresh` on the new tab, which flips the
        // view model to loading synchronously before the spawned task
        // resolves. With the mock service resolving almost instantly the
        // load may already be complete by the time we read state here, so
        // we accept either loading or loaded -- what we *do* assert is
        // that the context and status bar updated, and that the picker
        // closed. The Reloading state during the transition is covered by
        // the dedicated `startup_load_is_async_and_transitions_from_
        // loading_to_populated` test, which uses a gated service to
        // deterministically observe the loading window.
        shell.read_with(cx, |shell, cx| {
            assert_eq!(
                shell.context.region, "fleet",
                "the shell's context must track the picker's selection"
            );
            assert_eq!(
                shell.status_bar.read(cx).region,
                "fleet",
                "the status bar's region chip must show the new region"
            );
            assert_eq!(
                shell.status_bar.read(cx).active_quick_picker,
                None,
                "the picker must close once a row is selected"
            );
        });

        wait_until_context_settled(cx, &shell);
        let (loading, ready, has_instances) = shell.read_with(cx, |shell, cx| {
            let vm = &shell.ec2_tab.read(cx).list.read(cx).view_model;
            let loading = vm.is_loading();
            let status = shell.status_bar.read(cx).status.clone();
            let has_instances = !vm.instances().is_empty();
            (loading, status, has_instances)
        });
        assert!(
            !loading,
            "the context-switch load must settle back out of loading"
        );
        assert_eq!(
            ready,
            ContextStatusState::Ready,
            "the status bar must report Ready once the switched-in context has loaded"
        );
        assert!(
            has_instances,
            "the fleet view must show the selected profile's instances"
        );
    }

    /// Switching regions through the picker must clear the previously
    /// selected instance (the inspector closes) and show the standalone
    /// empty-state prompt in the list-only layout, since `apply_context`
    /// creates a fresh `EC2Tab` with a fresh view model.
    #[gpui::test]
    fn region_picker_switch_clears_selection_and_shows_standalone_prompt(cx: &mut TestAppContext) {
        let (shell, cx) = cx.add_window_view(|_, cx| AppShell::new_demo(cx));
        wait_until(cx, "startup load to settle", |cx| {
            cx.debug_bounds("ec2.instance.row.us-east-1:i-scenario-001")
                .is_some()
        });

        // Select an instance in the populated default region so the switch
        // has a selected instance to clear.
        let row = cx
            .debug_bounds("ec2.instance.row.us-east-1:i-scenario-001")
            .expect("a seeded instance row should render after the startup load");
        cx.simulate_click(row.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("ec2.tab.split").is_some(),
            "selecting a row should open the split (list + inspector) layout"
        );

        // Switch to the fleet region via the picker.
        let chip = cx
            .debug_bounds("statusbar.region")
            .expect("region chip should render");
        cx.simulate_click(chip.center(), gpui::Modifiers::default());
        let region_row = cx
            .debug_bounds("statusbar.quickpicker.region.row.fleet")
            .expect("fleet row should render in the demo's region picker");
        cx.simulate_click(region_row.center(), gpui::Modifiers::default());

        // After the switch: the old selection is gone (apply_context
        // creates a fresh EC2Tab with a fresh view model), the status bar
        // shows the fleet region, and the picker is closed.
        let (region, selection, picker_closed, standalone) = shell.read_with(cx, |shell, cx| {
            let vm = &shell.ec2_tab.read(cx).list.read(cx).view_model;
            let standalone = shell
                .ec2_tab
                .read(cx)
                .list
                .read(cx)
                .show_standalone_empty_state;
            (
                shell.context.region.clone(),
                vm.selected_instance_id().map(str::to_owned),
                shell.status_bar.read(cx).active_quick_picker.is_none(),
                standalone,
            )
        });
        assert_eq!(
            region, "fleet",
            "the shell's context must track the picker's region selection"
        );
        assert!(
            selection.is_none(),
            "the old selection must not survive a context switch"
        );
        assert!(
            picker_closed,
            "the picker must be closed after a region row is selected"
        );
        assert!(
            standalone,
            "with no selection after the switch the list-only (standalone prompt) layout is active"
        );

        wait_until_context_settled(cx, &shell);
        let (ready, has_instances) = shell.read_with(cx, |shell, cx| {
            let vm = &shell.ec2_tab.read(cx).list.read(cx).view_model;
            (
                shell.status_bar.read(cx).status.clone(),
                !vm.instances().is_empty(),
            )
        });
        assert!(
            matches!(ready, ContextStatusState::Ready),
            "the status bar must settle back to Ready after the region switch load"
        );
        assert!(has_instances, "the fleet region must show instances");
    }

    /// The empty-state path: a shell launched with the empty-fleet demo
    /// scenario shows the "No Instances Found" empty panel (not the
    /// loading or error panel) once the load settles, and the status bar
    /// reports Ready -- proving the empty state is visible after the
    /// initial load completes, not just during it.
    #[gpui::test]
    fn empty_demo_shows_empty_panel_and_ready_status(cx: &mut TestAppContext) {
        let (shell, cx) = cx.add_window_view(|_, cx| AppShell::new_demo_empty(cx));
        // The empty-fleet load resolves almost instantly; poll for the
        // empty panel to appear (it only renders once `is_loading` clears
        // and `instances` is empty).
        wait_until(cx, "empty panel to render", |cx| {
            cx.debug_bounds("ec2.instance.empty").is_some()
        });
        let (ready, instances_empty) = shell.read_with(cx, |shell, cx| {
            let vm = &shell.ec2_tab.read(cx).list.read(cx).view_model;
            (
                shell.status_bar.read(cx).status.clone(),
                vm.instances().is_empty(),
            )
        });
        assert!(
            matches!(ready, ContextStatusState::Ready),
            "the status bar must report Ready on an empty (but successful) load"
        );
        assert!(
            instances_empty,
            "the empty-fleet scenario must have no instances"
        );
    }

    /// The profile picker opens from the status bar chip and its rows are
    /// wired to the shell. Since the demo fixture has only one profile
    /// (already selected), the "All Profiles" section is empty — the test
    /// verifies the picker opens, the Current row renders, and the dismiss
    /// button closes the picker without changing state.
    #[gpui::test]
    fn profile_picker_opens_and_dismisses_without_changing_context(cx: &mut TestAppContext) {
        let (shell, cx) = cx.add_window_view(|_, cx| AppShell::new_demo(cx));
        wait_until(cx, "startup load to settle", |cx| {
            cx.debug_bounds("ec2.instance.row.us-east-1:i-scenario-001")
                .is_some()
        });

        let context_before = shell.read_with(cx, |shell, _| shell.context.clone());

        // Open the profile picker.
        let chip = cx
            .debug_bounds("statusbar.profile")
            .expect("profile chip should render");
        cx.simulate_click(chip.center(), gpui::Modifiers::default());
        assert!(
            cx.debug_bounds("statusbar.quickpicker").is_some(),
            "the quick picker popover should open from the profile chip click"
        );
        assert!(
            cx.debug_bounds("statusbar.quickpicker.profile.row.scenario-default")
                .is_some(),
            "the Current profile row should render in the picker"
        );

        // Dismiss without selecting.
        let dismiss = cx
            .debug_bounds("statusbar.quickpicker.dismiss")
            .expect("dismiss button should render");
        cx.simulate_click(dismiss.center(), gpui::Modifiers::default());

        let (picker_closed, context_after) = shell.read_with(cx, |shell, cx| {
            (
                shell.status_bar.read(cx).active_quick_picker.is_none(),
                shell.context.clone(),
            )
        });
        assert!(
            picker_closed,
            "dismissing the profile picker should close it"
        );
        assert_eq!(
            context_after, context_before,
            "dismissing the picker without selecting must not change the context"
        );
    }

    /// Selecting a *different* profile through the real picker must update
    /// the shell's context, the status bar's profile chip, close the
    /// picker, and replace the EC2Tab so the fleet reloads for the new
    /// profile. Injects a second profile into the status bar's picker data
    /// (the demo fixture only has one) so the "All Profiles" section has a
    /// selectable row.
    #[gpui::test]
    fn profile_picker_row_click_drives_shell_context(cx: &mut TestAppContext) {
        let (shell, cx) = cx.add_window_view(|_, cx| AppShell::new_demo(cx));
        wait_until(cx, "startup load to settle", |cx| {
            cx.debug_bounds("ec2.instance.row.us-east-1:i-scenario-001")
                .is_some()
        });

        // Inject a second profile into the status bar's picker data so
        // the "All Profiles" section has a selectable row. The demo
        // fixture only has `scenario-default`; this simulates a user who
        // has multiple AWS profiles configured.
        shell.update(cx, |shell, cx| {
            shell.status_bar.update(cx, |bar, cx| {
                bar.profiles = Some(vec![
                    crate::quick_picker::AwsProfile {
                        name: "scenario-default".into(),
                        hint: "us-east-1".into(),
                    },
                    crate::quick_picker::AwsProfile {
                        name: "other-profile".into(),
                        hint: "".into(),
                    },
                ]);
                cx.notify();
            });
        });
        cx.run_until_parked();

        // Open the profile picker.
        let chip = cx
            .debug_bounds("statusbar.profile")
            .expect("profile chip should render");
        cx.simulate_click(chip.center(), gpui::Modifiers::default());
        assert!(
            cx.debug_bounds("statusbar.quickpicker").is_some(),
            "the quick picker popover should open from the profile chip click"
        );
        assert!(
            cx.debug_bounds("statusbar.quickpicker.profile.row.other-profile")
                .is_some(),
            "the injected other-profile row should render in the All Profiles section"
        );

        // Click the other-profile row.
        let row = cx
            .debug_bounds("statusbar.quickpicker.profile.row.other-profile")
            .expect("other-profile row should be clickable");
        cx.simulate_click(row.center(), gpui::Modifiers::default());

        // Assert the shell's context, status bar, and picker state updated.
        let (context_profile, bar_profile, picker_closed) = shell.read_with(cx, |shell, cx| {
            (
                shell.context.profile.clone(),
                shell.status_bar.read(cx).profile.clone(),
                shell.status_bar.read(cx).active_quick_picker.is_none(),
            )
        });
        assert_eq!(
            context_profile, "other-profile",
            "the shell's context profile must match the picker's selection"
        );
        assert_eq!(
            bar_profile.as_deref(),
            Some("other-profile"),
            "the status bar's profile chip must show the selected profile"
        );
        assert!(
            picker_closed,
            "the picker must close once a profile row is selected"
        );

        // The EC2Tab was replaced by apply_context; the new view model
        // is loading or has already settled. Wait for the load to settle
        // (is_loading becomes false), then assert the fleet is empty
        // (the demo service for an unknown profile returns no instances).
        wait_until(cx, "profile switch load to settle", |cx| {
            cx.debug_bounds("ec2.instance.empty").is_some()
        });
        let (is_loading, instances_count, error) = shell.read_with(cx, |shell, cx| {
            let vm = &shell.ec2_tab.read(cx).list.read(cx).view_model;
            (
                vm.is_loading(),
                vm.instances().len(),
                vm.error_message().map(str::to_owned),
            )
        });
        assert!(!is_loading, "the profile switch load must have settled");
        assert_eq!(
            instances_count, 0,
            "the demo service for an unknown profile returns no instances"
        );
        assert!(
            error.is_none(),
            "the demo service for an unknown profile should not produce an error"
        );
    }

    /// Switching regions through the picker on a failure-scenario shell
    /// must show the error panel after the load settles. Uses the failure
    /// demo scenario (all loads fail with a simulated error) so the error
    /// state is deterministic.
    ///
    /// This test checks the *settled* error state after the region switch:
    /// the context updated, the picker closed, and the error panel
    /// rendered with the correct error message. The loading state during
    /// the transition (Reloading status bar, `is_loading` on the view
    /// model) is covered by
    /// `region_picker_switch_shows_reloading_state_during_failure_load`,
    /// which injects a `GatedComputeService` into `apply_context` to
    /// deterministically observe the loading window.
    #[gpui::test]
    fn region_picker_switch_shows_error_state_after_failure_load(
        cx: &mut TestAppContext,
    ) {
        let (shell, cx) = cx.add_window_view(|_, cx| AppShell::new_demo_failure(cx));

        // The failure scenario's startup load resolves almost instantly;
        // wait for the error panel to appear.
        wait_until(cx, "error panel to render after startup", |cx| {
            cx.debug_bounds("ec2.instance.error").is_some()
        });

        // The failure demo starts with region us-east-1 (the profile's
        // default region), so switching to fleet is a real change.
        let starting_region = shell.read_with(cx, |shell, _| shell.context.region.clone());
        assert_ne!(
            starting_region, "fleet",
            "the test must start in a concrete region so the fleet switch is a meaningful change"
        );

        // Open the region picker and switch to the fleet region.
        let chip = cx
            .debug_bounds("statusbar.region")
            .expect("region chip should render");
        cx.simulate_click(chip.center(), gpui::Modifiers::default());
        assert!(
            cx.debug_bounds("statusbar.quickpicker").is_some(),
            "the quick picker popover should open"
        );
        let region_row = cx
            .debug_bounds("statusbar.quickpicker.region.row.fleet")
            .expect("fleet row should render in the demo's region picker");
        cx.simulate_click(region_row.center(), gpui::Modifiers::default());

        // After the click: the context and status bar updated, the picker
        // closed. The new tab's load resolves almost instantly (mock
        // failure service), so the error may already be set.
        let (region, picker_closed) = shell.read_with(cx, |shell, cx| {
            (
                shell.context.region.clone(),
                shell.status_bar.read(cx).active_quick_picker.is_none(),
            )
        });
        assert_eq!(
            region, "fleet",
            "the shell's context must track the picker's region selection"
        );
        assert!(
            picker_closed,
            "the picker must be closed after a region row is selected"
        );

        // Wait for the new tab's view model to settle with an error
        // (the failure scenario always produces an error, so this is
        // deterministic). Poll the view model directly to avoid racing
        // with stale `debug_bounds` from the old tab.
        for _ in 0..200 {
            if shell.read_with(cx, |shell, cx| {
                shell
                    .ec2_tab
                    .read(cx)
                    .list
                    .read(cx)
                    .view_model
                    .error_message()
                    .is_some()
            }) {
                break;
            }
            cx.run_until_parked();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        // After settling: the view model has an error, the status bar
        // reports the error state, and the error message is the simulated
        // demo failure.
        let (has_error, status_after, error_msg) = shell.read_with(cx, |shell, cx| {
            let vm = &shell.ec2_tab.read(cx).list.read(cx).view_model;
            (
                vm.error_message().is_some(),
                shell.status_bar.read(cx).status.clone(),
                vm.error_message().map(str::to_owned),
            )
        });
        assert!(
            has_error,
            "the failure scenario must produce an error after the region switch"
        );
        assert!(
            matches!(status_after, ContextStatusState::Error(_)),
            "the status bar must report the error state after the failure load settles"
        );
        assert!(
            error_msg
                .as_deref()
                .unwrap_or("")
                .contains("Simulated demo load failure"),
            "the error message should be the simulated demo failure, got: {error_msg:?}"
        );
    }

    /// Switching regions through the picker while a `GatedComputeService`
    /// is injected into `apply_context` must show the Reloading state on
    /// the status bar and `is_loading` on the view model while the load
    /// is in flight, then the settled error state after the gate is
    /// released. This is the deterministic, picker-switch-specific version
    /// of the loading-window assertion that
    /// `startup_load_is_async_and_transitions_from_loading_to_populated`
    /// covers for the startup path.
    #[gpui::test]
    fn region_picker_switch_shows_reloading_state_during_failure_load(
        cx: &mut TestAppContext,
    ) {
        let gate = std::sync::Arc::new(tokio::sync::Notify::new());
        let (shell, cx) = cx.add_window_view(|_, cx| {
            let shell = AppShell::new_demo_failure(cx);
            shell
        });

        // The failure scenario's startup load resolves almost instantly
        // (MockComputeService); wait for the error panel to appear so the
        // shell is in a settled state before we inject the gated service.
        wait_until(cx, "error panel to render after startup", |cx| {
            cx.debug_bounds("ec2.instance.error").is_some()
        });

        // The failure demo starts with region us-east-1 (the profile's
        // default region), so switching to fleet is a real change.
        let starting_region = shell.read_with(cx, |shell, _| shell.context.region.clone());
        assert_ne!(
            starting_region, "fleet",
            "the test must start in a concrete region so the fleet switch is a meaningful change"
        );

        // Inject a GatedComputeService that will produce the same simulated
        // failure error as the failure scenario once the gate is released.
        // This overrides what `apply_context` would normally use
        // (MockComputeService) for the picker-driven context switch.
        shell.update(cx, |shell, _| {
            let service = GatedComputeService {
                gate: gate.clone(),
                instances: vec![],
                error: Some(
                    std::sync::Arc::new(std::io::Error::other(
                        "Simulated demo load failure",
                    )) as std::sync::Arc<dyn std::error::Error + Send + Sync>,
                ),
            };
            shell.set_test_service_override(std::sync::Arc::new(service));
        });

        // Open the region picker and switch to the fleet region. This
        // triggers `apply_context`, which uses the injected gated service.
        let chip = cx
            .debug_bounds("statusbar.region")
            .expect("region chip should render");
        cx.simulate_click(chip.center(), gpui::Modifiers::default());
        let region_row = cx
            .debug_bounds("statusbar.quickpicker.region.row.fleet")
            .expect("fleet row should render in the demo's region picker");
        cx.simulate_click(region_row.center(), gpui::Modifiers::default());

        // After the click: the context updated, the picker closed, and the
        // gated load is in flight (is_loading == true, status bar is
        // Reloading). The gate has not been released yet, so the error
        // state must NOT be visible.
        // Run one park cycle so `AppShell::render` has a chance to update
        // `status_bar.status` to `Reloading` based on the now-loading view
        // model (the `status` field is only set during render).
        cx.run_until_parked();
        let (region, picker_closed, is_loading, status_during) = shell.read_with(cx, |shell, cx| {
            let vm = &shell.ec2_tab.read(cx).list.read(cx).view_model;
            (
                shell.context.region.clone(),
                shell.status_bar.read(cx).active_quick_picker.is_none(),
                vm.is_loading(),
                shell.status_bar.read(cx).status.clone(),
            )
        });
        assert_eq!(
            region, "fleet",
            "the shell's context must track the picker's region selection"
        );
        assert!(
            picker_closed,
            "the picker must be closed after a region row is selected"
        );
        assert!(
            is_loading,
            "the view model must report loading while the gated load is in flight"
        );
        assert!(
            matches!(status_during, ContextStatusState::Reloading),
            "the status bar must report Reloading while the picker-driven load is in flight, got: {status_during:?}"
        );

        // Release the gate: the gated service now resolves with the
        // simulated failure error.
        gate.notify_one();

        // Wait for the load to settle with an error (poll the view model
        // directly to avoid racing with stale `debug_bounds` from the old
        // tab).
        for _ in 0..200 {
            if shell.read_with(cx, |shell, cx| {
                shell
                    .ec2_tab
                    .read(cx)
                    .list
                    .read(cx)
                    .view_model
                    .error_message()
                    .is_some()
            }) {
                break;
            }
            cx.run_until_parked();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        // After settling: the view model has the expected error, is no
        // longer loading, the status bar reports the error state, and the
        // error message is the simulated demo failure.
        let (has_error, is_loading_after, status_after, error_msg) = shell.read_with(cx, |shell, cx| {
            let vm = &shell.ec2_tab.read(cx).list.read(cx).view_model;
            (
                vm.error_message().is_some(),
                vm.is_loading(),
                shell.status_bar.read(cx).status.clone(),
                vm.error_message().map(str::to_owned),
            )
        });
        assert!(
            has_error,
            "the gated service must produce an error after the gate is released"
        );
        assert!(
            !is_loading_after,
            "the view model must no longer be loading after the gated load settles"
        );
        assert!(
            matches!(status_after, ContextStatusState::Error(_)),
            "the status bar must report the error state after the gated failure load settles, got: {status_after:?}"
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

        wait_until(cx, "startup load to settle", |cx| {
            cx.debug_bounds("ec2.instance.row.us-east-1:i-scenario-001")
                .is_some()
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
