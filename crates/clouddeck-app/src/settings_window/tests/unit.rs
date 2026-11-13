use super::*;

#[test]
fn match_ranges_are_case_insensitive_and_trimmed() {
    assert_eq!(
        match_ranges("Refresh interval, refresh", " REFRESH "),
        vec![0..7, 18..25]
    );
    assert!(match_ranges("Theme", "   ").is_empty());
}

#[test]
fn edit_ui_state_file_creates_missing_file() {
    let path = temp_settings_path().with_file_name("ui-state.json");
    edit_ui_state_file(&path, |state| state.sidebar_open = false).unwrap();
    assert!(!UiState::load(&path).sidebar_open);
    let _ = std::fs::remove_file(path);
}

#[test]
fn edit_ui_state_file_leaves_corrupt_file_untouched() {
    let path = temp_settings_path().with_file_name("ui-state.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"{not json").unwrap();
    let mut called = false;
    assert!(edit_ui_state_file(&path, |_| called = true).is_err());
    assert!(!called);
    assert_eq!(std::fs::read(&path).unwrap(), b"{not json");
    let _ = std::fs::remove_file(path);
}

#[test]
fn duration_settings_show_seconds_unit() {
    let kind = SettingKind::DurationSecs {
        min: None,
        max: None,
    };
    assert_eq!(unit_suffix(&kind), Some("seconds"));
}

fn setting(key: &str, group: Option<&str>) -> SettingDef {
    SettingDef {
        key: key.to_string(),
        label: key.to_string(),
        description: None,
        group: group.map(str::to_string),
        kind: SettingKind::Text,
        default: None,
        secret: false,
    }
}

fn category(settings: Vec<SettingDef>) -> SettingsCategory {
    SettingsCategory {
        extension: "aws".to_string(),
        name: "AWS".to_string(),
        settings,
    }
}

#[test]
fn provider_match_count_sums_settings_across_categories() {
    let hits = [
        category(vec![setting("a", None), setting("b", None)]),
        category(Vec::new()),
        category(vec![setting("c", None)]),
    ];
    assert_eq!(provider_match_count(&hits), 3);
    assert_eq!(provider_match_count(&[]), 0);
}

#[test]
fn field_key_pairs_extension_with_setting_key() {
    assert_eq!(
        field_key("aws", "region"),
        ("aws".to_string(), "region".to_string())
    );
}

#[test]
fn display_text_is_empty_without_a_value() {
    assert_eq!(display_text(None), "");
    assert_eq!(display_text(Some(SettingValue::Bool(true))), "true");
    assert_eq!(display_text(Some(SettingValue::DurationSecs(45))), "45");
}

#[test]
fn provider_groups_keep_first_appearance_order() {
    let defs = [
        setting("a", Some("Network")),
        setting("b", None),
        setting("c", Some("Network")),
        setting("d", Some("Auth")),
    ];
    let groups: Vec<(Option<&str>, Vec<&str>)> = provider_groups(&defs)
        .into_iter()
        .map(|(name, defs)| (name, defs.iter().map(|def| def.key.as_str()).collect()))
        .collect();
    assert_eq!(
        groups,
        vec![
            (Some("Network"), vec!["a", "c"]),
            (None, vec!["b"]),
            (Some("Auth"), vec!["d"]),
        ]
    );
}

#[test]
fn host_platform_matches_build_target() {
    let expected = if cfg!(target_os = "macos") {
        Platform::Mac
    } else {
        Platform::Linux
    };
    assert_eq!(host_platform(), expected);
}
