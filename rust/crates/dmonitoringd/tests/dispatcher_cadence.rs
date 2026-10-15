use openpilot_dmonitoringd::controller::{Controller, TOPICS};
use openpilot_messaging::state::{Options, State};
use openpilot_params::Params;
#[test]
fn missing_camera_dispatcher_keeps_publications_alive() {
    let dir = tempfile::tempdir().unwrap();
    let params = Params::open(dir.path(), "dm-cadence").unwrap();
    params.put_bool("DriverMonitoringEnabled", true).unwrap();
    let mut controller = Controller::new(&params).unwrap();
    let state = State::new(TOPICS, Options::default()).unwrap();
    assert!(
        controller.prepare(&state).unwrap().is_some(),
        "DM heartbeat must continue through driver camera absence"
    );
}
