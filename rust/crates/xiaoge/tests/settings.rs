use openpilot_xiaoge::settings::Settings;

#[test]
fn final_classifier_defaults_and_zero_confidence_thresholds_are_preserved() {
    let defaults = Settings::from_parameters(|_| None);
    assert_eq!(defaults.threshold, 0.94);
    assert_eq!(Settings::default().threshold, defaults.threshold);
    let zero = defaults
        .patch(&serde_json::json!({"threshold":0,"laneThreshold":0}))
        .unwrap();
    assert_eq!(zero.threshold, 0.0);
    assert_eq!(zero.lane_threshold, 0.0);
    assert_eq!(zero.parameters().unwrap()[0], ("OnnxBsdThreshold", 0));
    let clamped = Settings::from_parameters(|name| match name {
        "OnnxBsdThreshold" => Some(b"-5".to_vec()),
        "OnnxLaneThreshold" => Some(b"101".to_vec()),
        _ => None,
    });
    assert_eq!(clamped.threshold, 0.0);
    assert_eq!(clamped.lane_threshold, 1.0);
}
