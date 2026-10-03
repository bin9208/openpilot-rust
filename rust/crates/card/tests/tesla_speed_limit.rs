use capnp::message::Builder;
use openpilot_card::brands::tesla::{
    speed_limit::{create_wheel_frame, SpeedLimit},
    state::State,
    HAS_VEHICLE_BUS,
};
use openpilot_cereal::car_capnp::{car_control, car_state};
use std::path::Path;

const TEMPLATE: [u8; 8] = [0x29, 0x55, 0x77, 0xc0, 0x13, 0x24, 0x35, 0x80];
fn state() -> State {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../opendbc_repo/opendbc/dbc");
    let mut state = State::new(&root, HAS_VEHICLE_BUS, 12., true, 2_000_000_000).unwrap();
    let mut cs = state.out.get_root::<car_state::Builder>().unwrap();
    let mut cruise = cs.reborrow().init_cruise_state();
    cruise.set_enabled(true);
    cruise.set_speed_cluster(20.);
    state.extras.tesla_speed_limit_target = 25.;
    state.extras.tesla_speed_limit_target_nanos = 2_000_000_000;
    state.extras.tesla_speed_limit_target_valid = true;
    state.extras.tesla_speed_button_template = Some(TEMPLATE);
    state.extras.tesla_speed_button_template_nanos = 2_000_000_000;
    state
}
fn control() -> Builder<capnp::message::HeapAllocator> {
    let mut m = Builder::new_default();
    m.init_root::<car_control::Builder>().set_enabled(true);
    m
}

#[test]
fn speed_wheel_tick_retains_every_unrelated_template_bit() {
    let up = create_wheel_frame(&TEMPLATE, 1).unwrap();
    let down = create_wheel_frame(&TEMPLATE, -1).unwrap();
    assert_eq!(up, [0x29, 0x55, 0x77, 0xc1, 0x13, 0x24, 0x35, 0x80]);
    assert_eq!(down, [0x29, 0x55, 0x77, 0xff, 0x13, 0x24, 0x35, 0x80]);
}
#[test]
fn non_idle_template_is_rejected() {
    let mut template = TEMPLATE;
    template[3] |= 1;
    assert!(create_wheel_frame(&template, 1).is_err());
}
#[test]
fn automatic_speed_waits_for_target_stability_then_feedback() {
    let mut state = state();
    let cc = control();
    let mut controller = SpeedLimit {
        configured: true,
        ..SpeedLimit::default()
    };
    assert!(controller
        .update(cc.get_root_as_reader().unwrap(), &state, 2_000_000_000)
        .unwrap()
        .is_empty());
    state.extras.tesla_speed_limit_target_nanos = 2_500_000_000;
    let frames = controller
        .update(cc.get_root_as_reader().unwrap(), &state, 2_500_000_000)
        .unwrap();
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].address, 0x3c2);
    assert_eq!(frames[0].data[3], 0xc1);
    state.extras.tesla_speed_limit_target_nanos = 2_900_000_000;
    assert!(controller
        .update(cc.get_root_as_reader().unwrap(), &state, 2_900_000_000)
        .unwrap()
        .is_empty());
}
#[test]
fn feedback_timeout_blocks_same_target_and_current_signature() {
    let mut state = state();
    let cc = control();
    let mut controller = SpeedLimit {
        configured: true,
        ..SpeedLimit::default()
    };
    controller
        .update(cc.get_root_as_reader().unwrap(), &state, 2_000_000_000)
        .unwrap();
    state.extras.tesla_speed_limit_target_nanos = 2_500_000_000;
    controller
        .update(cc.get_root_as_reader().unwrap(), &state, 2_500_000_000)
        .unwrap();
    state.extras.tesla_speed_limit_target_nanos = 3_700_000_000;
    assert!(controller
        .update(cc.get_root_as_reader().unwrap(), &state, 3_700_000_000)
        .unwrap()
        .is_empty());
    assert_eq!(controller.feedback_blocked_signature, Some((90, 72)));
}
#[test]
fn wheel_resume_gesture_waits_for_idle_before_counting_again() {
    let mut state = state();
    let mut up = TEMPLATE;
    up[3] |= 1;
    let mut down = TEMPLATE;
    down[3] |= 63;
    state.observe_speed_wheel(&up, 2_000_000_000);
    state.observe_speed_wheel(&down, 2_010_000_000);
    state.observe_speed_wheel(&up, 2_020_000_000);
    assert_eq!(state.extras.tesla_manual_speed_adjustment_counter, 2);
    assert_eq!(state.extras.tesla_speed_auto_resume_gesture_counter, 1);
    state.observe_speed_wheel(&TEMPLATE, 2_030_000_000);
    state.observe_speed_wheel(&up, 2_040_000_000);
    assert_eq!(state.extras.tesla_manual_speed_adjustment_counter, 3);
}
