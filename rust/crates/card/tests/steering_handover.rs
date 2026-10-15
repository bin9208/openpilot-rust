use openpilot_card::brands::hyundai::steering_handover;
use steering_handover::{Input, SteeringHandover};
fn compare(actual: &serde_json::Value, expected: &serde_json::Value, path: &str) {
    match (actual, expected) {
        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            assert_eq!(a.to_bits(), b.to_bits(), "{path}: {a} != {b}");
        }
        (serde_json::Value::Array(a), serde_json::Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}/{i}"));
            }
        }
        (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (k, a) in a {
                compare(a, &b[k], &format!("{path}/{k}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
#[test]
fn complete_pinned_source_48_regression_tests_match_native_handover_state_and_caps() {
    let data = zstd::stream::decode_all(
        include_bytes!("fixtures/steering-handover-final.json.zst").as_slice(),
    )
    .unwrap();
    let fixture: serde_json::Value = serde_json::from_slice(&data).unwrap();
    let mut states = std::collections::BTreeMap::new();
    for (i, row) in fixture["rows"].as_array().unwrap().iter().enumerate() {
        let state = states
            .entry(row["instance"].as_u64().unwrap())
            .or_insert_with(SteeringHandover::default);
        let mut input = row["input"].clone();
        let nan_driver = input["driver"] == "NaN";
        if nan_driver {
            input["driver"] = serde_json::json!(0.);
        }
        let mut input: Input = serde_json::from_value(input).unwrap();
        if nan_driver {
            input.driver = f64::NAN;
        }
        let result = state.update(input);
        compare(
            &serde_json::json!(result),
            &row["result"],
            &format!("result/{i}"),
        );
        compare(
            &serde_json::to_value(state).unwrap(),
            &row["state"],
            &format!("state/{i}"),
        );
    }
    assert_eq!(fixture["rows"].as_array().unwrap().len(), 13484);
}

#[test]
fn pinned_controller_model_clock_preserves_150ms_boundary_missing_and_invalid_samples() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/handover-model-final.json")).unwrap();
    let mut clock = steering_handover::ModelFreshness::default();
    for (i, row) in fixture["rows"].as_array().unwrap().iter().enumerate() {
        let frame = row["frame"].as_u64().map(|v| u32::try_from(v).unwrap());
        let std = if row["std"] == "NaN" {
            Some(f64::NAN)
        } else {
            row["std"].as_f64()
        };
        assert_eq!(
            clock.update(frame, std, row["now"].as_u64().unwrap()),
            row["result"].as_bool().unwrap(),
            "valid/{i}"
        );
        assert_eq!(
            serde_json::json!(clock.frame),
            row["model_frame"],
            "frame/{i}"
        );
        assert_eq!(clock.time, row["model_time"].as_u64().unwrap(), "time/{i}");
    }
}
