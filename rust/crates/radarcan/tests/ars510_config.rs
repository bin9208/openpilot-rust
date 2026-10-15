use openpilot_radarcan::decoder::toyota::ars510::config::NativeInterfaceConfig;
#[test]
fn pinned_native_fused_and_explicit_experimental_profile_defaults_match() {
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ars510-config-final.json")).unwrap();
    let actual = serde_json::json!({"default":NativeInterfaceConfig::default(),"all_tracks":NativeInterfaceConfig::all_tracks(),"base":NativeInterfaceConfig::base(),"fused":NativeInterfaceConfig::fused(),"colored":NativeInterfaceConfig::colored()});
    assert_eq!(actual, expected);
}
