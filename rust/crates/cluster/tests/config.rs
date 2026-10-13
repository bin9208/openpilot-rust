use openpilot_cluster::config::{dark_theme, light_theme, EGO_FORWARD_M, PATH_START_M};

#[test]
fn source_theme_colors_match() {
    let actual = serde_json::json!({"light": light_theme(), "dark": dark_theme()});
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("data/config_themes.json")).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(PATH_START_M.to_bits(), EGO_FORWARD_M.to_bits());
}
