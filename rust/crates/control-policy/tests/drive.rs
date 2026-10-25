use openpilot_control_policy::drive::apply_deadzone;

#[test]
fn deadzone_subtracts_width_outside_and_includes_both_edges() {
    // 고정 이진 분수 입력과 기대값으로 원본의 양쪽 경계 및 연속성을 확인한다.
    for (error, expected) in [
        (-1., -0.5),
        (-0.75, -0.25),
        (-0.5, 0.),
        (-0.25, 0.),
        (0., 0.),
        (0.25, 0.),
        (0.5, 0.),
        (0.75, 0.25),
        (1., 0.5),
    ] {
        assert_eq!(apply_deadzone(error, 0.5), expected, "error={error}");
    }
}

#[test]
fn deadzone_zero_width_preserves_nonzero_error_and_returns_positive_zero() {
    assert_eq!(apply_deadzone(-0.75, 0.), -0.75);
    assert_eq!(apply_deadzone(0.75, 0.), 0.75);
    for error in [-0., 0.] {
        assert_eq!(apply_deadzone(error, 0.).to_bits(), 0.0_f64.to_bits());
    }
}

#[test]
fn deadzone_keeps_source_branch_order_for_negative_width() {
    // 원본은 폭을 절댓값으로 바꾸거나 음수 폭을 거부하지 않는다.
    for (error, expected) in [
        (-0.75, -1.25),
        (-0.5, -1.),
        (-0.25, 0.25),
        (0., 0.5),
        (0.25, 0.75),
        (0.5, 1.),
        (0.75, 1.25),
    ] {
        assert_eq!(apply_deadzone(error, -0.5), expected, "error={error}");
    }
}

#[test]
fn deadzone_keeps_source_comparisons_for_nonfinite_inputs() {
    assert_eq!(apply_deadzone(f64::INFINITY, 0.5), f64::INFINITY);
    assert_eq!(apply_deadzone(f64::NEG_INFINITY, 0.5), f64::NEG_INFINITY);
    assert_eq!(apply_deadzone(f64::NAN, 0.5), 0.);
    assert_eq!(apply_deadzone(0.75, f64::NAN), 0.);
    assert_eq!(apply_deadzone(0.75, f64::INFINITY), 0.);
    assert_eq!(apply_deadzone(f64::INFINITY, f64::INFINITY), 0.);
    assert_eq!(apply_deadzone(f64::NEG_INFINITY, f64::INFINITY), 0.);
}
