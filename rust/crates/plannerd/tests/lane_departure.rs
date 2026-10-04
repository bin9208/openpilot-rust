use openpilot_plannerd::lane_departure::{LaneDeparture, LaneDepartureInput, Prediction};

fn enabled() -> LaneDepartureInput {
    LaneDepartureInput {
        frame: 500,
        speed: 20.,
        left_blinker: false,
        right_blinker: false,
        lateral_active: false,
        prediction: Some(Prediction {
            left_probability: 0.2,
            right_probability: 0.3,
            left_visibility: 0.6,
            right_visibility: 0.7,
            left_y: -1.,
            right_y: 1.,
        }),
    }
}

#[test]
fn warnings_start_at_source_frame_cooldown() {
    // Given: source uses DT_CTRL even though the caller increments on model frames.
    let mut warning = LaneDeparture::default();
    let mut input = enabled();
    input.frame = 499;
    // When: the initial 500-frame cooldown expires.
    let before = warning.update(&input);
    input.frame = 500;
    let at = warning.update(&input);
    // Then: both source thresholds become eligible at the exact boundary.
    assert_eq!((before.left, before.right), (false, false));
    assert_eq!((at.left, at.right), (true, true));
}

#[test]
fn blinker_restarts_both_warning_cooldowns() {
    // Given: a right indicator at model frame 700.
    let mut warning = LaneDeparture::default();
    let mut input = enabled();
    input.frame = 700;
    input.right_blinker = true;
    warning.update(&input);
    input.right_blinker = false;
    // When: the model stream reaches frames immediately before and at expiry.
    input.frame = 1199;
    let before = warning.update(&input);
    input.frame = 1200;
    let at = warning.update(&input);
    // Then: either indicator suppresses both sides for exactly 500 frames.
    assert_eq!((before.left, before.right), (false, false));
    assert_eq!((at.left, at.right), (true, true));
}

#[test]
fn missing_prediction_clears_previous_warning() {
    // Given: two active departure warnings.
    let mut warning = LaneDeparture::default();
    let mut input = enabled();
    warning.update(&input);
    input.prediction = None;
    // When: the model provides no desire prediction.
    let result = warning.update(&input);
    // Then: stale warnings are cleared.
    assert_eq!((result.left, result.right), (false, false));
}

#[test]
fn strict_lane_thresholds_preserve_camera_offset() {
    // Given: left/right distances equal the camera-offset boundaries.
    let mut warning = LaneDeparture::default();
    let mut input = enabled();
    let prediction = input.prediction.as_mut().unwrap();
    prediction.left_y = -(1.08 + 0.04);
    prediction.right_y = 1.08 - 0.04;
    // When: eligible predictions reach those boundaries.
    let result = warning.update(&input);
    // Then: equality does not trigger a departure.
    assert_eq!((result.left, result.right), (false, false));
}
