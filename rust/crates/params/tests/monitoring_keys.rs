use openpilot_params::{
    metadata, Params, CLEAR_ON_IGNITION_ON, CLEAR_ON_MANAGER_START, CLEAR_ON_ONROAD_TRANSITION,
};

#[test]
fn session_disable_survives_onroad_transitions_but_not_ignition_or_manager_restart() {
    let root = tempfile::tempdir().unwrap();
    let params = Params::open(root.path(), "d").unwrap();
    params.put_bool("DriverMonitoringEnabled", true).unwrap();
    params
        .put_bool("DriverMonitoringSessionDisabled", true)
        .unwrap();
    params.clear(CLEAR_ON_ONROAD_TRANSITION).unwrap();
    assert!(params.get_bool("DriverMonitoringSessionDisabled").unwrap());
    for flag in [CLEAR_ON_IGNITION_ON, CLEAR_ON_MANAGER_START] {
        params
            .put_bool("DriverMonitoringSessionDisabled", true)
            .unwrap();
        params.clear(flag).unwrap();
        assert!(!params.get_bool("DriverMonitoringSessionDisabled").unwrap());
        assert!(params.get_bool("DriverMonitoringEnabled").unwrap());
    }
    assert_eq!(
        metadata("DriverMonitoringEnabled").unwrap().default,
        Some("1")
    );
    assert_eq!(metadata("DriverMonitoringMode").unwrap().default, Some("0"));
    assert_eq!(metadata("CarrotVisionEnabled").unwrap().default, Some("0"));
}
