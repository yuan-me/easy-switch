use easy_switch_core::Settings;

#[test]
fn old_settings_gain_appearance_defaults_and_round_trip() {
    let settings: Settings = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
    assert_eq!(settings.accent_color, "#007acc");
    assert_eq!(settings.color_style, "vivid");
    assert_eq!(settings.theme, "dark");
    settings.validate_appearance().unwrap();
    let mut changed=settings;
    changed.accent_color="#12ABef".into();
    changed.color_style="soft".into();
    let json=serde_json::to_value(&changed).unwrap();
    assert_eq!(json["accentColor"],"#12ABef");
    assert_eq!(json["colorStyle"],"soft");
    serde_json::from_value::<Settings>(json).unwrap().validate_appearance().unwrap();
}

#[test]
fn reject_invalid_appearance_values() {
    let mut settings=Settings::default();
    for color in ["red", "#fff", "#12345g", "#123456;", "#紫紫"] {
        settings.accent_color=color.into();
        assert!(settings.validate_appearance().is_err());
    }
    settings.accent_color="#000000".into();
    settings.color_style="unknown".into();
    assert!(settings.validate_appearance().is_err());
}
