//! carrot's `tools/jetlink/test_blinker_tbt.py`.
use super::{
    observed::Meta,
    tbt::{candidate, Nav, TbtBlinker, TbtCar, TbtInput, TBT_DISTANCE_MAX_M},
    Side,
};
use openpilot_desire::types::{Car, Model};

struct Scene {
    car: Car,
    tbt: TbtCar,
    model: Model,
    meta: Meta,
    nav: Nav,
    enabled: bool,
    valid: bool,
    manual: bool,
}

fn scene() -> Scene {
    Scene {
        car: Car {
            v_ego: 20.,
            right_lane_line: 10,
            ..Car::default()
        },
        tbt: TbtCar {
            rear_inputs_available: true,
            right_rear_blocked: false,
            right_blindspot: false,
        },
        model: Model {
            lane_line_probs: [0.9; 4],
            lane_lines: [-5.4, -1.8, 1.8, 5.4].map(|y| vec![y; 9]),
            road_edges: [vec![-8.; 9], vec![8.; 9]],
            ..Model::default()
        },
        meta: Meta {
            lane_change_available_right: true,
            lane_width_right: 3.5,
            lane_change_state: "off",
            lane_change_direction: "none",
            desire: "none",
            ..Meta::default()
        },
        nav: Nav {
            turn_info: 2,
            distance: 230.,
            atc_type: "none".into(),
        },
        enabled: true,
        valid: true,
        manual: false,
    }
}

fn step(t: &mut TbtBlinker, s: &Scene, now: f64) -> Option<(&'static str, Side)> {
    t.update(TbtInput {
        now,
        enabled: s.enabled,
        valid: s.valid,
        car: &s.car,
        tbt: s.tbt,
        model: Some((&s.model, &s.meta)),
        nav: Some(&s.nav),
        manual: s.manual,
        busy: false,
        front_clear: true,
        distance_limit: TBT_DISTANCE_MAX_M,
    })
}

fn arm(t: &mut TbtBlinker, s: &Scene, start: f64) -> Option<(&'static str, Side)> {
    let mut result = None;
    for i in 0..22 {
        result = step(t, s, start + f64::from(i) * 0.05);
    }
    result
}

#[test]
fn ineligible_conditions_never_light() {
    let cases: [fn(&mut Scene); 16] = [
        |s| s.nav.turn_info = 1,
        |s| s.nav.turn_info = 3,
        |s| s.nav.distance = 323.,
        |s| s.nav.distance = 0.,
        |s| s.nav.distance = f64::NAN,
        |s| s.car.v_ego = 2.5,
        |s| s.car.drive = false,
        |s| s.car.can_valid = false,
        |s| s.car.trailer_connected = true,
        |s| s.car.steering_pressed = true,
        |s| s.tbt.rear_inputs_available = false,
        |s| s.tbt.right_rear_blocked = true,
        |s| s.car.right_lane_line = 11,
        |s| s.car.right_lane_line = 20,
        |s| s.car.right_lane_line = -1,
        |s| s.car.steer_fault_temporary = true,
    ];
    for (index, case) in cases.iter().enumerate() {
        let mut s = scene();
        case(&mut s);
        assert!(
            arm(&mut TbtBlinker::default(), &s, 10.).is_none(),
            "case {index}"
        );
    }
}

#[test]
fn lane_required_not_just_road_edge() {
    let mut s = scene();
    s.model.lane_line_probs[3] = 0.1;
    assert!(!candidate(
        &s.car,
        s.tbt,
        &s.model,
        &s.meta,
        &s.nav,
        TBT_DISTANCE_MAX_M
    ));
    assert!(arm(&mut TbtBlinker::default(), &s, 10.).is_none());
}

#[test]
fn enabled_fresh_and_stable_required() {
    for field in 0..2 {
        let mut s = scene();
        if field == 0 {
            s.enabled = false;
        } else {
            s.valid = false;
        }
        assert!(arm(&mut TbtBlinker::default(), &s, 10.).is_none());
    }
    let (mut t, s) = (TbtBlinker::default(), scene());
    assert!(step(&mut t, &s, 10.).is_none());
    assert!(
        step(&mut t, &s, 11.).is_none(),
        "a gap cannot count as confirmation"
    );
    assert_eq!(arm(&mut t, &s, 11.05), Some(("lane", Side::Right)));
}

#[test]
fn cancel_before_start_does_not_retrigger_same_tbt() {
    for event in 0..5 {
        let (mut t, mut s) = (TbtBlinker::default(), scene());
        assert_eq!(arm(&mut t, &s, 10.), Some(("lane", Side::Right)));
        match event {
            0 => s.car.steering_pressed = true,
            1 => s.tbt.right_rear_blocked = true,
            2 => s.valid = false,
            3 => s.manual = true,
            _ => s.car.left_blinker = true,
        }
        assert!(step(&mut t, &s, 11.1).is_none());
        let s = scene();
        for i in 0..50 {
            assert!(
                step(&mut t, &s, 11.15 + f64::from(i) * 0.05).is_none(),
                "event {event}"
            );
        }
    }
}

#[test]
fn repeat_only_after_finish_lamps_off_and_fresh_confirmation() {
    let (mut t, mut s) = (TbtBlinker::default(), scene());
    assert_eq!(arm(&mut t, &s, 10.), Some(("lane", Side::Right)));
    s.car.right_blinker = true;
    s.meta.lane_change_state = "laneChangeStarting";
    s.meta.lane_change_direction = "right";
    s.meta.lane_change_available_right = false;
    assert_eq!(step(&mut t, &s, 11.1), Some(("lane", Side::Right)));
    s.meta.lane_change_state = "laneChangeFinishing";
    assert_eq!(step(&mut t, &s, 11.15), Some(("lane", Side::Right)));
    s.meta.lane_change_state = "preLaneChange";
    assert!(step(&mut t, &s, 11.2).is_none());
    for i in 0..50 {
        assert!(step(&mut t, &s, 11.25 + f64::from(i) * 0.05).is_none());
    }
    s.car.right_blinker = false;
    s.meta.lane_change_state = "off";
    s.meta.lane_change_available_right = true;
    assert_eq!(arm(&mut t, &s, 13.75), Some(("lane", Side::Right)));
}

#[test]
fn interrupted_maneuver_and_start_timeout_cancel() {
    let (mut t, mut s) = (TbtBlinker::default(), scene());
    arm(&mut t, &s, 10.);
    s.meta.lane_change_state = "laneChangeStarting";
    s.meta.lane_change_direction = "right";
    assert!(step(&mut t, &s, 11.1).is_some());
    s.meta.lane_change_state = "off";
    assert!(step(&mut t, &s, 11.15).is_none());
    assert_eq!(t.phase, "cancelled");

    let (mut t, s) = (TbtBlinker::default(), scene());
    arm(&mut t, &s, 10.);
    for i in 0..100 {
        step(&mut t, &s, 11.1 + f64::from(i) * 0.05);
    }
    assert_eq!(t.phase, "cancelled");
}

#[test]
fn fixed_250_metre_boundary() {
    for kph in [30., 60., 100.] {
        let mut s = scene();
        s.car.v_ego = kph / 3.6;
        s.nav.distance = 250.;
        assert!(candidate(
            &s.car,
            s.tbt,
            &s.model,
            &s.meta,
            &s.nav,
            TBT_DISTANCE_MAX_M
        ));
        s.nav.distance = 250.1;
        assert!(!candidate(
            &s.car,
            s.tbt,
            &s.model,
            &s.meta,
            &s.nav,
            TBT_DISTANCE_MAX_M
        ));
    }
}
