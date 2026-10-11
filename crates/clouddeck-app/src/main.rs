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
mod ec2_tab;
mod failure_popover;
mod fleet_state;
mod icons;
mod inspector_pane;
mod instance_detail;
mod instance_list;
mod instance_table;
mod linux_titlebar;
mod main_window;
mod profile_picker_view;
mod quick_picker;
mod region_picker_view;
mod row_context_menu;
mod runtime;
mod search_field;
mod settings_window;
mod shell;
mod sidebar;
mod status_badge;
mod status_bar;
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

use gpui::{App, Application, Bounds, actions, prelude::*, px, size};

use shell::AppShell;

actions!(clouddeck, [Quit]);

fn main() {
    Application::new()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            gpui_component::init(cx);
            // SurgicalTheme is intentionally light. Keep gpui-component inputs and
            // popovers on the same palette even when macOS is using dark mode.
            gpui_component::Theme::change(gpui_component::ThemeMode::Light, None, cx);
            let bounds = Bounds::centered(None, size(px(1280.0), px(800.0)), cx);
            cx.open_window(main_window::window_options(bounds), |window, cx| {
                let shell = cx.new(|cx| AppShell::new(window, cx));
                cx.new(|cx| gpui_component::Root::new(shell, window, cx))
            })
            .unwrap();
            cx.on_action(|_: &Quit, cx| cx.quit());
            cx.bind_keys(shell::app_key_bindings());
            cx.activate(true);
        });
}
