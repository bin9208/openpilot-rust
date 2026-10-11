use openpilot_cluster::autorun::params::Reader;
use openpilot_params::Params;

#[test]
fn autorun_settings_preserve_source_integer_and_gate_rules() {
    let temporary = tempfile::tempdir().unwrap();
    let params = Params::open(temporary.path(), "fixture").unwrap();
    for (key, value) in [
        ("ClusterHud", b"1tail".as_slice()),
        ("ClusterHudEncoder", b"44".as_slice()),
        ("ClusterHudOrientation", b"2tail".as_slice()),
        ("ClusterHudDebug", b"-1".as_slice()),
        ("IsOnroad", b"1".as_slice()),
    ] {
        params.put(key, value).unwrap();
    }
    let reader = Reader::new(params);
    let settings = reader.settings();
    assert_eq!(settings.hud_mode, 1);
    assert_eq!(settings.encoder.setting(), 0);
    assert_eq!(settings.orientation, Some(2));
    assert!(reader.output_allowed());
}

#[test]
fn autorun_retains_its_source_params_owner_after_namespace_removal() {
    let temporary = tempfile::tempdir().unwrap();
    let params = Params::open(temporary.path(), "fixture").unwrap();
    params.put("ClusterHud", b"1").unwrap();
    let reader = Reader::new(params);
    std::fs::remove_file(temporary.path().join("fixture")).unwrap();
    assert_eq!(reader.settings().hud_mode, 0);
    assert!(!temporary.path().join("fixture").exists());
}
