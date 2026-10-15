use openpilot_manager::parameters::{configure_monitoring, set_defaults};
use openpilot_params::Params;

#[test]
fn legacy_mode_only_migrates_streaming_and_new_settings_remain_live() {
    for (legacy, expected_streaming) in [
        (b"0".as_slice(), false),
        (b"1", false),
        (b"2", true),
        (b" 2 trailing", true),
    ] {
        let root = tempfile::tempdir().unwrap();
        let params = Params::open(root.path(), "d").unwrap();
        params.put("DisableDM", legacy).unwrap();
        configure_monitoring(&params).unwrap();
        set_defaults(&params, false).unwrap();
        assert!(params.get_bool("DriverMonitoringEnabled").unwrap());
        assert_eq!(
            params.get_bool("CarrotVisionEnabled").unwrap(),
            expected_streaming
        );
        assert_eq!(params.get("DriverMonitoringMode").unwrap().unwrap(), b"0");
        params.put("DriverMonitoringMode", b"1").unwrap();
        params
            .put_bool("CarrotVisionEnabled", !expected_streaming)
            .unwrap();
        configure_monitoring(&params).unwrap();
        assert_eq!(params.get("DriverMonitoringMode").unwrap().unwrap(), b"1");
        assert_eq!(
            params.get_bool("CarrotVisionEnabled").unwrap(),
            !expected_streaming
        );
    }
}

#[test]
fn migration_respects_existing_streaming_and_does_not_commit_after_invalid_legacy_input() {
    let root = tempfile::tempdir().unwrap();
    let params = Params::open(root.path(), "d").unwrap();
    params.put("DisableDM", b"invalid").unwrap();
    assert!(configure_monitoring(&params).is_err());
    assert_eq!(params.get("DriverMonitoringMode").unwrap(), None);
    params.put_bool("CarrotVisionEnabled", false).unwrap();
    configure_monitoring(&params).unwrap();
    assert!(!params.get_bool("CarrotVisionEnabled").unwrap());
}
