#[test]
fn native_scalar_clip_matches_pinned_float_clip_including_nan_payload_and_signed_zero() {
    let source: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/hyundai-scalar-final.json")).unwrap();
    for (i, row) in source["rows"].as_array().unwrap().iter().enumerate() {
        let values = row["input"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| f64::from_bits(v.as_u64().unwrap()))
            .collect::<Vec<_>>();
        let actual = openpilot_control_policy::math::clip(values[0], values[1], values[2]);
        assert_eq!(
            actual.to_bits(),
            row["expected"].as_u64().unwrap(),
            "source bit pattern {i}"
        );
    }
}
