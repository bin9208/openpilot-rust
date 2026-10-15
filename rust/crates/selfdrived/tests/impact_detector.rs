use openpilot_selfdrived::impact_detector::ImpactDetector;
#[test]
fn final_source_imu_trace_preserves_sign_gravity_calibration_and_freshness() {
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/impact-detector-final.json")).unwrap();
    let mut detector = ImpactDetector::default();
    for (i, row) in reference["cases"].as_array().unwrap().iter().enumerate() {
        let vector = |name: &str| {
            row[name]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect::<Vec<_>>()
        };
        let sensor = vector("sensor");
        let orientation = vector("orientation");
        let calibration = vector("calibration");
        let actual = detector.update(
            row["now"].as_f64().unwrap(),
            row["timestamp"].as_f64().unwrap(),
            &sensor,
            &orientation,
            &calibration,
            row["valid"].as_bool().unwrap(),
        );
        assert_eq!(
            actual.trigger,
            row["trigger"].as_bool().unwrap(),
            "trigger {i}"
        );
        assert_eq!(actual.quiet, row["quiet"].as_bool(), "quiet {i}");
        if row["acceleration"].is_null() {
            assert!(actual.acceleration.is_none());
        } else {
            for (a, b) in actual
                .acceleration
                .unwrap()
                .iter()
                .zip(vector("acceleration"))
            {
                assert!((a - b).abs() < 1e-11, "acceleration {i}: {a} {b}");
            }
        }
    }
}
