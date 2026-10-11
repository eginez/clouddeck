mod account_connect;
mod account_window;
mod app_menus;
mod assets;
mod center_state_view;
mod command_palette;
mod context_bar;
// `account_provider_step`, `account_profile_picker`, and
// `account_workspace_setup` (the old multi-step onboarding wizard views)
// were removed under clouddeck-rs-w6s.18: the approved single-screen
// Connect to AWS design (there is effectively only one provider, AWS, so a
// provider-selection step added nothing) folded profile/region selection
// directly into `account_window::AccountWindowView`. Nothing outside their
// own files referenced them.
mod failure_popover;
mod fleet_state;
mod icons;
mod inspector_pane;
mod instance_table;
mod linux_titlebar;
mod main_window;
mod profile_picker_view;
mod region_picker_view;
mod row_context_menu;
mod runtime;
mod search_field;
mod settings_window;
mod shell;
mod sidebar;
#[cfg(test)]
mod test_support;
mod theme;
mod tokens;
mod toolbar;
// `sidebar` (the port of the oracle's left-nav `SidebarView`) was removed:
// the agreed layout (clouddeck-rs-wz2.3) drops the left sidebar in favor of
// a full-width central table with a bottom context bar. `shell.rs` already
// composes the shell without it; see
// `AppShell::render`'s `table_and_status_are_docked_without_a_sidebar` test.
mod view_models;
mod welcome_window;

use clouddeck_core::{WorkspaceContext, context_path};
use gpui::{App, Application, actions};

actions!(clouddeck, [Quit]);

fn main() {
    Application::new()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            gpui_component::init(cx);
            // Light until a MainWindow syncs the theme (main_window.rs apply_appearance).
            // The welcome window never does, so this default is what it renders with.
            gpui_component::Theme::change(gpui_component::ThemeMode::Light, None, cx);
            let (demo, demo_scenario) = shell::launch_options();
            let saved = WorkspaceContext::load(&context_path());
            if welcome_window::should_show_welcome(demo, saved.as_ref()) {
                welcome_window::open_welcome_window(cx).expect("welcome window opens");
            } else {
                shell::open_main_window(cx, demo, demo_scenario).expect("main window opens");
            }
            cx.on_action(|_: &Quit, cx| cx.quit());
            cx.bind_keys(shell::app_key_bindings());
            cx.activate(true);
        });
}
