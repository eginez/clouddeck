//! Actions, key bindings and the native menu bar.

use crate::{Quit, update_window};
use gpui::{App, KeyBinding, Menu, MenuItem, actions};

actions!(
    clouddeck,
    [
        SwitchProfile,
        SwitchRegion,
        RefreshFleet,
        OpenCommandPalette,
        ToggleSidebar,
        ToggleInspector,
        CopyInstanceId,
        OpenInConsole,
        OpenSettings,
        CheckForUpdates,
    ]
);

pub fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("secondary-p", SwitchProfile, None),
        KeyBinding::new("secondary-shift-r", SwitchRegion, None),
        KeyBinding::new("secondary-r", RefreshFleet, None),
        KeyBinding::new("secondary-shift-p", OpenCommandPalette, None),
        KeyBinding::new("secondary-b", ToggleSidebar, None),
        KeyBinding::new("secondary-alt-i", ToggleInspector, None),
        KeyBinding::new("secondary-c", CopyInstanceId, Some("InstanceTable")),
        KeyBinding::new("secondary-o", OpenInConsole, None),
        KeyBinding::new("secondary-,", OpenSettings, None),
        KeyBinding::new("secondary-q", Quit, None),
    ]
}

pub fn app_menus() -> Vec<Menu> {
    vec![
        Menu {
            name: "CloudDeck".into(),
            items: vec![
                MenuItem::action("Check for Updates…", CheckForUpdates),
                MenuItem::action("Settings…", OpenSettings),
                MenuItem::separator(),
                MenuItem::action("Quit CloudDeck", Quit),
            ],
        },
        Menu {
            name: "View".into(),
            items: vec![
                MenuItem::action("Toggle Sidebar", ToggleSidebar),
                MenuItem::action("Toggle Inspector", ToggleInspector),
                MenuItem::separator(),
                MenuItem::action("Command Palette…", OpenCommandPalette),
            ],
        },
        Menu {
            name: "Fleet".into(),
            items: vec![
                MenuItem::action("Refresh", RefreshFleet),
                MenuItem::action("Switch Profile…", SwitchProfile),
                MenuItem::action("Switch Region…", SwitchRegion),
            ],
        },
        Menu {
            name: "Instance".into(),
            items: vec![
                MenuItem::action("Copy Instance ID", CopyInstanceId),
                MenuItem::action("Open in Console", OpenInConsole),
            ],
        },
    ]
}

pub fn install(cx: &mut App) {
    cx.bind_keys(key_bindings());
    cx.set_menus(app_menus());
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &CheckForUpdates, cx| update_window::check_for_updates(cx));
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Action, TestAppContext};

    fn expected_action_names() -> Vec<&'static str> {
        vec![
            SwitchProfile::name_for_type(),
            SwitchRegion::name_for_type(),
            RefreshFleet::name_for_type(),
            OpenCommandPalette::name_for_type(),
            ToggleSidebar::name_for_type(),
            ToggleInspector::name_for_type(),
            CopyInstanceId::name_for_type(),
            OpenInConsole::name_for_type(),
            OpenSettings::name_for_type(),
            Quit::name_for_type(),
        ]
    }

    #[test]
    fn every_action_is_bound_exactly_once() {
        let bindings = key_bindings();
        assert_eq!(bindings.len(), 10);

        let mut bound: Vec<&str> = bindings.iter().map(|b| b.action().name()).collect();
        bound.sort_unstable();
        let mut expected = expected_action_names();
        expected.sort_unstable();
        assert_eq!(bound, expected);
    }

    #[test]
    fn menu_bar_has_expected_menus_and_fleet_items() {
        let menus = app_menus();
        let names: Vec<&str> = menus.iter().map(|m| m.name.as_ref()).collect();
        assert_eq!(names, ["CloudDeck", "View", "Fleet", "Instance"]);

        let fleet = menus.iter().find(|m| m.name == "Fleet").unwrap();
        assert_eq!(fleet.items.len(), 3);
    }

    #[test]
    fn check_for_updates_sits_directly_above_settings() {
        let menus = app_menus();
        let app_menu = menus.iter().find(|m| m.name == "CloudDeck").unwrap();
        let names: Vec<&str> = app_menu
            .items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Action { name, .. } => Some(name.as_ref()),
                _ => None,
            })
            .collect();

        let updates = names
            .iter()
            .position(|name| *name == "Check for Updates…")
            .expect("Check for Updates… is in the app menu");
        assert_eq!(names[updates + 1], "Settings…");
    }

    #[gpui::test]
    fn install_registers_bindings_and_menus(cx: &mut TestAppContext) {
        cx.update(install);
    }
}
