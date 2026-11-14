mod about_window;
mod app_menus;
mod app_providers;
mod assets;
mod center_state_view;
mod command_palette;
mod context_bar;
mod dimension_picker_view;
mod failure_popover;
mod fleet_state;
mod icons;
mod inspector_pane;
mod instance_table;
#[cfg(test)]
mod layout_check;
mod linux_titlebar;
mod main_window;
mod plugins_window;
mod profile_picker_view;
mod provider_call;
mod provider_picker_view;
mod provider_sign_in;
mod render_state;
mod row_context_menu;
mod runtime;
mod search_field;
mod setting_rows;
mod settings_window;
mod shell;
mod sidebar;
#[cfg(test)]
mod test_support;
mod theme;
mod tokens;
mod toolbar;
mod ui_record;
mod update_window;
mod updater;
// `sidebar` (the port of the oracle's left-nav `SidebarView`) was removed:
// the agreed layout (clouddeck-rs-wz2.3) drops the left sidebar in favor of
// a full-width central table with a bottom context bar. `shell.rs` already
// composes the shell without it; see
// `AppShell::render`'s `table_and_status_are_docked_without_a_sidebar` test.
#[cfg(test)]
mod context_bar_provider_tests;
mod onboarding_window;
#[cfg(test)]
mod view_models;

use clouddeck_core::{DemoFleet, WorkspaceContext, context_path};
use gpui::{App, Application, WindowHandle, actions};
use gpui_component::Root;
use onboarding_window::OnboardingEvent;

actions!(clouddeck, [Quit]);

fn should_show_onboarding(demo: bool, saved: Option<&WorkspaceContext>) -> bool {
    !demo && saved.is_none()
}

fn open_onboarding(cx: &mut App, provider: Option<&str>) -> Option<WindowHandle<Root>> {
    onboarding_window::open_onboarding_window(cx, provider, None, |event, cx| match event {
        OnboardingEvent::SignedIn { .. } => {
            shell::open_main_window(cx, false, DemoFleet::Happy);
        }
        OnboardingEvent::Open {
            provider,
            connection,
        } => {
            let context = WorkspaceContext {
                provider,
                profile: connection.as_str().to_string(),
            };
            if let Err(error) = context.save(&shell::saved_context_path()) {
                eprintln!("could not remember the chosen profile: {error}");
            }
            shell::open_main_window(cx, false, DemoFleet::Happy);
        }
        OnboardingEvent::ExploreDemo => {
            shell::open_main_window(cx, true, DemoFleet::Happy);
        }
    })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("extension") {
        std::process::exit(clouddeck_core::extension_manager::cli::run(
            &args[2..],
            &mut std::io::stdout(),
        ));
    }
    Application::new()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            gpui_component::init(cx);
            // Light until a MainWindow syncs the theme (main_window.rs apply_appearance).
            gpui_component::Theme::change(gpui_component::ThemeMode::Light, None, cx);
            let options = shell::launch_options();
            app_providers::install(cx);
            if let Err(error) =
                clouddeck_core::migrate_aws_settings(&clouddeck_core::settings_path())
            {
                eprintln!("could not move AWS settings into the AWS plugin: {error}");
            }
            let saved = WorkspaceContext::load(&context_path());
            if let Some(state) = options.state.filter(|state| state.is_about()) {
                if state == render_state::State::AboutDark {
                    cx.set_global(tokens::ActivePalette(tokens::Palette::for_dark(true)));
                }
                about_window::open(cx).expect("about window opens");
            } else if let Some(state) = options.state.filter(|state| state.is_plugins()) {
                if state == render_state::State::PluginsDark {
                    cx.set_global(tokens::ActivePalette(tokens::Palette::for_dark(true)));
                }
                plugins_window::open(cx).expect("plugins window opens");
            } else if let Some(provider) = options.onboarding.as_ref() {
                open_onboarding(cx, provider.as_deref()).expect("onboarding window opens");
            } else if !(should_show_onboarding(options.demo, saved.as_ref())
                && open_onboarding(cx, None).is_some())
            {
                shell::open_main_window_with(
                    cx,
                    options.demo,
                    options.scenario,
                    options.size,
                    options.state,
                )
                .expect("main window opens");
            }
            cx.on_action(|_: &Quit, cx| cx.quit());
            cx.bind_keys(shell::app_key_bindings());
            updater::start(cx);
            cx.activate(true);
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_show_onboarding_false_for_saved_gcp_context() {
        let saved = WorkspaceContext {
            provider: "gcp".into(),
            profile: "team".into(),
        };
        assert!(!should_show_onboarding(false, Some(&saved)));
    }
}
