use openpilot_modeld::action::{
    dynamic_lat_smooth_seconds, from_plan, Action, ActionInputs, PlanActionInput,
};

fn inputs() -> ActionInputs {
    ActionInputs {
        lat_action_t: 0.5,
        long_action_t: 0.5,
        v_ego: 10.0,
        lat_smooth_seconds: 0.0,
        v_ego_stopping: 0.05,
    }
}

fn constant_plan(speed: f32) -> [[f32; 15]; 33] {
    let mut plan = [[0.0; 15]; 33];
    for row in &mut plan {
        row[3] = speed;
        row[11] = 0.25;
        row[14] = 0.125;
    }
    plan
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
}

#[test]
fn holds_curvature_when_speed_is_at_lateral_boundary() {
    // Given
    let plan = constant_plan(1.0);
    let previous = Action {
        desired_curvature: -0.02,
        ..Action::default()
    };
    let inputs = ActionInputs {
        v_ego: 0.3,
        ..inputs()
    };
    // When
    let action = from_plan(
        PlanActionInput {
            plan: &plan,
            direct_action: Some([5.0, 0.0]),
        },
        previous,
        inputs,
    );
    // Then
    close(action.desired_curvature, -0.02);
}

#[test]
fn uses_curvature_when_speed_exceeds_lateral_boundary() {
    // Given
    let plan = constant_plan(1.0);
    let inputs = ActionInputs {
        v_ego: 0.300_000_001,
        ..inputs()
    };
    // When
    let action = from_plan(
        PlanActionInput {
            plan: &plan,
            direct_action: None,
        },
        Action::default(),
        inputs,
    );
    // Then: curvature uses the minimum 1 m/s divisor.
    close(action.desired_curvature, 0.875);
}

#[test]
fn interpolates_plan_when_action_time_is_between_samples() {
    // Given: t[4] = 0.15625 and t[5] = 0.244140625.
    let mut plan = constant_plan(2.0);
    plan[0][6] = 0.5;
    plan[4][3] = 3.0;
    plan[5][3] = 5.0;
    plan[4][11] = 0.1;
    plan[5][11] = 0.3;
    let inputs = ActionInputs {
        lat_action_t: 0.2001953125,
        long_action_t: 0.2001953125,
        ..inputs()
    };
    // When
    let action = from_plan(
        PlanActionInput {
            plan: &plan,
            direct_action: None,
        },
        Action::default(),
        inputs,
    );
    // Then
    let alpha = 1.0 - (-0.05_f64 / 0.3).exp();
    close(
        action.desired_acceleration,
        alpha * (4.0 / 0.2001953125 - 0.5),
    );
    close(
        action.desired_curvature,
        (f64::from(0.1_f32) + f64::from(0.3_f32)) / (10.0 * 0.2001953125) - 0.0125,
    );
}

#[test]
fn keeps_stop_decision_when_direct_action_overrides_acceleration() {
    // Given
    let plan = constant_plan(0.0);
    let previous = Action {
        desired_acceleration: -1.0,
        ..Action::default()
    };
    // When
    let action = from_plan(
        PlanActionInput {
            plan: &plan,
            direct_action: Some([2.0, 3.0]),
        },
        previous,
        inputs(),
    );
    // Then
    close(action.desired_curvature, f64::from(0.02_f32));
    close(
        action.desired_acceleration,
        -1.0 + 4.0 * (1.0 - (-0.05_f64 / 0.3).exp()),
    );
    assert!(action.should_stop);
}

#[test]
fn does_not_stop_when_target_equals_stopping_threshold() {
    // Given
    let plan = constant_plan(0.25);
    let inputs = ActionInputs {
        v_ego_stopping: 0.25,
        ..inputs()
    };
    // When
    let action = from_plan(
        PlanActionInput {
            plan: &plan,
            direct_action: None,
        },
        Action::default(),
        inputs,
    );
    // Then
    assert!(!action.should_stop);
}

#[test]
fn does_not_stop_when_one_second_target_is_moving() {
    // Given: t[13] exceeds 1.5 seconds, so the one-second preview is moving.
    let mut plan = constant_plan(0.0);
    plan[13][3] = 1.0;
    // When
    let action = from_plan(
        PlanActionInput {
            plan: &plan,
            direct_action: None,
        },
        Action::default(),
        inputs(),
    );
    // Then
    assert!(!action.should_stop);
}

#[test]
fn smooths_maximum_velocity_when_peak_is_after_action_time() {
    // Given
    let mut plan = constant_plan(1.0);
    plan[32][3] = 9.0;
    let previous = Action {
        desired_velocity: 4.0,
        ..Action::default()
    };
    // When
    let action = from_plan(
        PlanActionInput {
            plan: &plan,
            direct_action: None,
        },
        previous,
        inputs(),
    );
    // Then
    close(
        action.desired_velocity,
        4.0 + 5.0 * (1.0 - (-0.05_f64 / 0.3).exp()),
    );
}

#[test]
fn smooths_curvature_when_lateral_tau_is_positive() {
    // Given
    let plan = constant_plan(1.0);
    let previous = Action {
        desired_curvature: -0.1,
        ..Action::default()
    };
    let inputs = ActionInputs {
        lat_smooth_seconds: 0.2,
        ..inputs()
    };
    // When
    let action = from_plan(
        PlanActionInput {
            plan: &plan,
            direct_action: Some([2.0, 0.0]),
        },
        previous,
        inputs,
    );
    // Then
    close(
        action.desired_curvature,
        -0.1 + (f64::from(0.02_f32) + 0.1) * (1.0 - (-0.25_f64).exp()),
    );
}

#[test]
fn clamps_interpolation_when_action_time_exceeds_horizon() {
    // Given
    let mut plan = constant_plan(1.0);
    plan[32][3] = 3.0;
    let inputs = ActionInputs {
        long_action_t: 12.0,
        ..inputs()
    };
    // When
    let action = from_plan(
        PlanActionInput {
            plan: &plan,
            direct_action: None,
        },
        Action::default(),
        inputs,
    );
    // Then
    close(
        action.desired_acceleration,
        (1.0 - (-0.05_f64 / 0.3).exp()) / 3.0,
    );
}

#[test]
fn preserves_dynamic_smoothing_fallback_when_plan_std_index_is_invalid() {
    // Given
    let cases = [(-0.1, 0.0), (0.0, 0.0), (0.2, 0.2), (0.6, 0.6), (0.9, 0.6)];
    // When
    let results = cases.map(|(base, expected)| (dynamic_lat_smooth_seconds(base), expected));
    // Then
    for (actual, expected) in results {
        close(actual, expected);
    }
}

#[test]
fn rounds_direct_curvature_when_speed_squared_is_not_exactly_float32() {
    // Given
    let plan = constant_plan(1.0);
    let inputs = ActionInputs {
        v_ego: 17.3,
        ..inputs()
    };
    // When
    let action = from_plan(
        PlanActionInput {
            plan: &plan,
            direct_action: Some([2.0, 0.0]),
        },
        Action::default(),
        inputs,
    );
    // Then: actual NumPy 2 scalar division rounds the denominator and quotient.
    close(action.desired_curvature, 0.006682481616735458);
}

#[test]
fn uses_exact_action_sample_despite_nonfinite_predecessor() {
    for predecessor in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut plan = constant_plan(0.0);
        plan[4][3] = predecessor;
        plan[4][11] = predecessor;
        let action = from_plan(
            PlanActionInput {
                plan: &plan,
                direct_action: None,
            },
            Action::default(),
            ActionInputs {
                lat_action_t: 0.244140625,
                long_action_t: 0.244140625,
                ..inputs()
            },
        );
        close(action.desired_curvature, 0.1923);
        close(action.desired_acceleration, 0.0);
        assert!(action.should_stop);
    }
}

#[test]
fn preserves_infinite_action_endpoints_between_samples() {
    for value in [f32::INFINITY, f32::NEG_INFINITY] {
        for same_endpoint in [false, true] {
            let mut plan = constant_plan(0.0);
            plan[4][3] = value;
            plan[4][11] = value;
            if same_endpoint {
                plan[5][3] = value;
                plan[5][11] = value;
            }
            let action = from_plan(
                PlanActionInput {
                    plan: &plan,
                    direct_action: None,
                },
                Action::default(),
                ActionInputs {
                    lat_action_t: 0.2,
                    long_action_t: 0.2,
                    ..inputs()
                },
            );
            assert_eq!(action.desired_curvature, f64::from(value));
            assert_eq!(action.desired_acceleration, f64::from(value));
            assert_eq!(action.should_stop, value.is_sign_negative());
        }
    }
}

#[test]
fn preserves_negative_infinity_in_one_second_stop_preview() {
    let mut plan = constant_plan(0.0);
    plan[11][3] = f32::NEG_INFINITY;
    let action = from_plan(
        PlanActionInput {
            plan: &plan,
            direct_action: None,
        },
        Action::default(),
        ActionInputs {
            long_action_t: 0.2,
            ..inputs()
        },
    );
    assert!(action.should_stop);
}
