use openpilot_card::state_helpers::{parse_gear, Blinkers, SpeedFilter, SteeringPressed};
use openpilot_cereal::car_capnp::car_state::GearShifter;

#[test]
fn speed_reset_and_acceleration_preserve_source_filter_state() {
    let mut filter = SpeedFilter::new().unwrap();
    assert_eq!(filter.update(10.), [10., 0.]);
    let next = filter.update(11.);
    assert_eq!(next, [10.174060389135185, 1.6592563982783979]);
    assert_eq!(filter.update(0.), [0., 0.]);
}

#[test]
fn blinker_edges_and_pressed_threshold_use_source_cadence() {
    let mut blinkers = Blinkers::default();
    assert_eq!(blinkers.lamp(3, true, false), [true, false]);
    assert_eq!(blinkers.lamp(3, false, false), [true, false]);
    assert_eq!(blinkers.stalk(3, false, true), [false, true]);
    assert_eq!(blinkers.stalk(3, false, false), [false, true]);
    assert_eq!(blinkers.stalk(3, false, false), [false, false]);
    let mut pressed = SteeringPressed::default();
    assert!(!pressed.update(true, 2));
    assert!(!pressed.update(true, 2));
    assert!(pressed.update(true, 2));
    assert!(!pressed.update(false, 2));
    assert_eq!(parse_gear(Some("dRiVe")), GearShifter::Drive);
    assert_eq!(parse_gear(Some("")), GearShifter::Unknown);
}
