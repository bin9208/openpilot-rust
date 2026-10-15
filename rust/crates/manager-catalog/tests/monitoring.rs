use openpilot_manager_catalog::{Predicate, State, SystemGpsPaths};
use openpilot_params::Params;

#[test]
fn monitoring_dispatcher_survives_disable_and_streaming_is_independent_of_hud() {
    let root = tempfile::tempdir().unwrap();
    let mut params = Params::open(root.path(), "d").unwrap();
    params.put("DisableDM", b"2").unwrap();
    let onroad = State {
        started: true,
        not_car: false,
    };
    assert!(Predicate::DriverMonitoring
        .evaluate(onroad, &mut params, &mut SystemGpsPaths)
        .unwrap());
    params.put_bool("CarrotVisionEnabled", true).unwrap();
    params.put("ClusterHud", b"1").unwrap();
    assert!(Predicate::WebRtc
        .evaluate(onroad, &mut params, &mut SystemGpsPaths)
        .unwrap());
}

#[test]
fn camera_model_stops_for_disabled_session_but_driver_view_overrides_it() {
    let root = tempfile::tempdir().unwrap();
    let mut params = Params::open(root.path(), "d").unwrap();
    params.put_bool("DriverMonitoringEnabled", true).unwrap();
    let onroad = State {
        started: true,
        not_car: false,
    };
    assert!(Predicate::DriverMonitoringModel
        .evaluate(onroad, &mut params, &mut SystemGpsPaths)
        .unwrap());
    params
        .put_bool("DriverMonitoringSessionDisabled", true)
        .unwrap();
    assert!(!Predicate::DriverMonitoringModel
        .evaluate(onroad, &mut params, &mut SystemGpsPaths)
        .unwrap());
    assert!(Predicate::DriverMonitoring
        .evaluate(onroad, &mut params, &mut SystemGpsPaths)
        .unwrap());
    params.put_bool("IsDriverViewEnabled", true).unwrap();
    let offroad = State {
        started: false,
        not_car: false,
    };
    assert!(Predicate::DriverMonitoringModel
        .evaluate(offroad, &mut params, &mut SystemGpsPaths)
        .unwrap());
}
