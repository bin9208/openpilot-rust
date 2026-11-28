//! Ported from carrot-wip `opendbc/car/tesla/tests/test_speed_limit_controller.py`.
use super::*;
use crate::core::Message;
use std::path::Path;

const IDLE_TEMPLATE: [u8; 8] = [0x29, 0x55, 0, 0, 0, 0, 0, 0x80];
const START: u64 = 2_000_000_000;

fn controller() -> SpeedLimit {
    SpeedLimit {
        configured: true,
        ..SpeedLimit::default()
    }
}
/// carrot's `fake_state`: cruise on at 20 m/s, a 25 m/s limit, a fresh template.
fn fake_state() -> State {
    let dbc = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../opendbc_repo/opendbc/dbc");
    let mut state = State::new(&dbc, 0, 15., true, 0).unwrap();
    let mut cs = state.out.init_root::<car_state::Builder>();
    cs.set_can_valid(true);
    let mut cruise = cs.init_cruise_state();
    cruise.set_enabled(true);
    cruise.set_speed_cluster(20.);
    state.extras.tesla_speed_limit_target = 25.;
    state.extras.tesla_speed_limit_target_nanos = START;
    state.extras.tesla_speed_limit_target_valid = true;
    state.extras.tesla_speed_button_template = Some(IDLE_TEMPLATE);
    state.extras.tesla_speed_button_template_nanos = START;
    state
}
fn control(active_carrot: i16) -> Message {
    let mut message = Message::new_default();
    let mut cc = message.init_root::<car_control::Builder>();
    cc.set_enabled(true);
    cc.reborrow()
        .init_hud_control()
        .set_active_carrot(active_carrot);
    message
}
fn update(controller: &mut SpeedLimit, cc: &Message, state: &State, now: u64) -> Vec<Frame> {
    controller
        .update(cc.get_root_as_reader().unwrap(), state, now)
        .unwrap()
}

#[test]
fn speed_wheel_frame_only_changes_signed_tick() {
    assert_eq!(
        create_wheel_frame(&IDLE_TEMPLATE, 1).unwrap(),
        [0x29, 0x55, 0, 1, 0, 0, 0, 0x80]
    );
    assert_eq!(
        create_wheel_frame(&IDLE_TEMPLATE, -1).unwrap(),
        [0x29, 0x55, 0, 0x3f, 0, 0, 0, 0x80]
    );
}

#[test]
fn controller_waits_for_stable_target_and_feedback() {
    let mut controller = controller();
    let mut state = fake_state();
    let cc = control(0);
    assert!(update(&mut controller, &cc, &state, START).is_empty());
    let sends = update(&mut controller, &cc, &state, 2_500_000_000);
    assert_eq!(sends.len(), 1);
    assert_eq!(sends[0].address, 0x3c2);
    assert_eq!(sends[0].bus, 1);
    assert_eq!(sends[0].data, [0x29, 0x55, 0, 1, 0, 0, 0, 0x80]);
    state.extras.tesla_speed_limit_target_nanos = 2_900_000_000;
    assert!(update(&mut controller, &cc, &state, 2_900_000_000).is_empty());

    state
        .out
        .get_root::<car_state::Builder>()
        .unwrap()
        .get_cruise_state()
        .unwrap()
        .set_speed_cluster((20. + 1. / 3.6) as f32);
    state.extras.tesla_speed_limit_target_nanos = 3_100_000_000;
    assert_eq!(update(&mut controller, &cc, &state, 3_100_000_000).len(), 1);
}

#[test]
fn controller_waits_while_carrot_speed_control_is_active() {
    for active_carrot in 2..=6 {
        let mut controller = controller();
        let mut state = fake_state();
        assert!(update(&mut controller, &control(active_carrot), &state, START).is_empty());
        state.extras.tesla_speed_limit_target_nanos = 3_000_000_000;
        assert!(update(
            &mut controller,
            &control(active_carrot),
            &state,
            3_000_000_000
        )
        .is_empty());
        state.extras.tesla_speed_limit_target_nanos = 3_100_000_000;
        assert!(update(&mut controller, &control(1), &state, 3_100_000_000).is_empty());
        state.extras.tesla_speed_limit_target_nanos = 3_600_000_000;
        state.extras.tesla_speed_button_template_nanos = 3_600_000_000;
        assert_eq!(
            update(&mut controller, &control(1), &state, 3_600_000_000).len(),
            1
        );
    }
}

#[test]
fn invalid_measurements_and_untrusted_timestamps_never_produce_speed_ticks() {
    for gate in [
        "sensors",
        "can",
        "future_target",
        "zero_target",
        "future_template",
        "zero_template",
    ] {
        let mut controller = controller();
        let mut state = fake_state();
        let cc = control(0);
        assert!(update(&mut controller, &cc, &state, START).is_empty());
        state.extras.tesla_speed_limit_target_nanos = 2_500_000_000;
        let mut cs = state.out.get_root::<car_state::Builder>().unwrap();
        cs.set_vehicle_sensors_invalid(gate == "sensors");
        cs.set_can_valid(gate != "can");
        match gate {
            "future_target" => state.extras.tesla_speed_limit_target_nanos += 1,
            "zero_target" => state.extras.tesla_speed_limit_target_nanos = 0,
            "future_template" => state.extras.tesla_speed_button_template_nanos = 2_500_000_001,
            "zero_template" => state.extras.tesla_speed_button_template_nanos = 0,
            _ => {}
        }
        assert!(
            update(&mut controller, &cc, &state, 2_500_000_000).is_empty(),
            "{gate}"
        );
        assert_eq!(controller.pending_direction, 0, "{gate}");
    }
}

#[test]
fn every_gate_open_sends_the_tick_the_gates_withhold() {
    let mut controller = controller();
    let mut state = fake_state();
    let cc = control(0);
    assert!(update(&mut controller, &cc, &state, START).is_empty());
    state.extras.tesla_speed_limit_target_nanos = 2_500_000_000;
    assert_eq!(update(&mut controller, &cc, &state, 2_500_000_000).len(), 1);
}
