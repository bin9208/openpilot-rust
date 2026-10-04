use openpilot_plannerd::{
    model::Model,
    turn_accel::{future_curvature, limit, TurnInput},
};

fn input() -> TurnInput {
    TurnInput {
        speed: 20.,
        curvature: 0.,
        acceleration: [-2., 1.6],
        lateral_maximum: 3.,
        safety_ratio: 0.70,
        minimum_speed: 0.1,
        cruise_speed: Some(25.),
        current_curvature: 0.,
    }
}

#[test]
fn missing_model_velocity_preserves_curvature_fallback() {
    // Given: an incomplete model prediction.
    let model = Model::default();
    // When: querying its upcoming curvature.
    let value = future_curvature(&model, -0.003, 1.).unwrap();
    // Then: the control curvature fallback remains unchanged.
    assert_eq!(value, -0.003);
}

#[test]
fn excessive_current_lateral_acceleration_caps_only_positive_thrust() {
    // Given: current curvature already exceeds the combined comfort envelope.
    let mut input = input();
    input.current_curvature = 0.01;
    // When: limiting the acceleration plan without model geometry.
    let result = limit(&input, None).unwrap();
    // Then: positive thrust is removed while the braking bound is preserved.
    assert_eq!(result, [-2., 0.]);
}
