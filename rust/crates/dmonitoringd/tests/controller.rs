use openpilot_cereal::log_capnp::event;
use openpilot_dmonitoringd::controller::{Controller, TOPICS};
use openpilot_messaging::state::{Options, State};
use openpilot_params::Params;
fn decode(bytes: &[u8]) -> (bool, bool, bool) {
    let message = capnp::serialize::read_message(
        std::io::Cursor::new(bytes),
        capnp::message::ReaderOptions::new(),
    )
    .unwrap();
    let event::DriverMonitoringState(dm) = message
        .get_root::<event::Reader>()
        .unwrap()
        .which()
        .unwrap()
    else {
        panic!()
    };
    let dm = dm.unwrap();
    (
        dm.get_dm2_disabled(),
        dm.get_camera_unavailable(),
        dm.get_no_response_force_decel(),
    )
}
#[test]
fn missing_camera_still_publishes_and_disabled_mode_suppresses_force_decel() {
    let dir = tempfile::tempdir().unwrap();
    let params = Params::open(dir.path(), "dm").unwrap();
    params.put_bool("DriverMonitoringEnabled", true).unwrap();
    let mut controller = Controller::new(&params).unwrap();
    let state = State::new(TOPICS, Options::default()).unwrap();
    let output = controller.prepare_at(&state, 1.).unwrap().unwrap();
    assert_eq!(decode(&output.bytes), (false, true, false));
    params
        .put_bool("DriverMonitoringSessionDisabled", true)
        .unwrap();
    // Live enable is sampled every forty dispatcher ticks, independent of camera frame IDs.
    for i in 1..40 {
        controller.prepare_at(&state, 1. + i as f64 * 0.05).unwrap();
    }
    controller.refresh(&params).unwrap();
    let output = controller.prepare_at(&state, 3.).unwrap().unwrap();
    assert_eq!(decode(&output.bytes), (true, false, false));
}
fn car(now: f64, hold: i16) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut root = message.init_root::<event::Builder>();
    root.set_valid(true);
    root.set_log_mono_time((now * 1e9) as u64);
    let mut cs = root.init_car_state();
    cs.set_can_valid(true);
    cs.set_gear_shifter(openpilot_cereal::car_capnp::car_state::GearShifter::Drive);
    cs.set_standstill(true);
    cs.set_soft_hold_active(hold);
    capnp::serialize::write_message_to_words(&message)
}
fn selfdrive(now: f64) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut root = message.init_root::<event::Builder>();
    root.set_valid(true);
    root.set_log_mono_time((now * 1e9) as u64);
    let mut sd = root.init_selfdrive_state();
    sd.set_enabled(true);
    sd.set_active(true);
    capnp::serialize::write_message_to_words(&message)
}
#[test]
fn stationary_soft_hold_freezes_monitor_without_changing_session_enable() {
    let dir = tempfile::tempdir().unwrap();
    let params = Params::open(dir.path(), "dm-hold").unwrap();
    params.put_bool("DriverMonitoringEnabled", true).unwrap();
    let mut controller = Controller::new(&params).unwrap();
    controller.monitoring.monitor.awareness = 0.4;
    controller.monitoring.monitor.alert_3_cnt = 1;
    let mut state = State::new(
        TOPICS,
        Options {
            simulation: true,
            ignore_frequency: TOPICS.iter().map(|v| v.to_string()).collect(),
            ..Options::default()
        },
    )
    .unwrap();
    for i in 0..30 {
        let now = 1. + i as f64 * 0.05;
        state.update(now, &[car(now, 2), selfdrive(now)]).unwrap();
        let output = controller.prepare_at(&state, now).unwrap().unwrap();
        assert!(decode(&output.bytes).0);
    }
    assert_eq!(controller.monitoring.monitor.awareness, 0.4);
    assert_eq!(controller.monitoring.monitor.alert_3_cnt, 1);
    assert!(!params.get_bool("DriverMonitoringSessionDisabled").unwrap());
    assert!(params.get_bool("DriverMonitoringEnabled").unwrap());
    // A stale hold is no longer eligible to suppress assessment.
    state.update(3., &[]).unwrap();
    let output = controller.prepare_at(&state, 3.).unwrap().unwrap();
    assert!(!decode(&output.bytes).0);
}
#[test]
fn three_physical_cancel_edges_disable_only_current_session() {
    let dir = tempfile::tempdir().unwrap();
    let params = Params::open(dir.path(), "dm-cancel").unwrap();
    params.put_bool("DriverMonitoringEnabled", true).unwrap();
    let mut controller = Controller::new(&params).unwrap();
    for i in 0..6 {
        let now = 1. + i as f64 * 0.1;
        let mut message = capnp::message::Builder::new_default();
        let mut root = message.init_root::<event::Builder>();
        root.set_valid(true);
        root.set_log_mono_time((now * 1e9) as u64);
        let mut cs = root.init_car_state();
        cs.set_can_valid(true);
        let mut button = cs.init_button_events(1).get(0);
        button.set_type(openpilot_cereal::car_capnp::car_state::button_event::Type::Cancel);
        button.set_pressed(i % 2 == 0);
        controller
            .car_packet(
                &capnp::serialize::write_message_to_words(&message),
                now,
                &params,
            )
            .unwrap();
    }
    assert!(params.get_bool("DriverMonitoringSessionDisabled").unwrap());
    assert!(params.get_bool("DriverMonitoringEnabled").unwrap());
}
