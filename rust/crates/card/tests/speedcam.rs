#[path = "../src/brands/hyundai/navigation/speedcam.rs"]
mod speedcam;
use speedcam::SpeedcamPolicy;
fn compare(actual: &serde_json::Value, expected: &serde_json::Value, path: &str) {
    match (actual, expected) {
        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            assert_eq!(a.to_bits(), b.to_bits(), "{path}: {a:?} != {b:?}");
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
fn complete_pinned_source_24_regressions_match_native_camera_policy_transitions() {
    let data =
        zstd::stream::decode_all(include_bytes!("fixtures/speedcam-final.json.zst").as_slice())
            .unwrap();
    let fixture: serde_json::Value = serde_json::from_slice(&data).unwrap();
    for (i, row) in fixture["rows"].as_array().unwrap().iter().enumerate() {
        let mut state: SpeedcamPolicy = serde_json::from_value(row["before"].clone()).unwrap();
        let input = &row["input"];
        let n = |key: &str| input[key].as_f64().unwrap();
        let b = |key: &str| input[key].as_bool().unwrap();
        let result = match row["operation"].as_str().unwrap() {
            "reset_warning" => {
                state.reset_warning();
                serde_json::Value::Null
            }
            "clear" => {
                state.clear();
                serde_json::Value::Null
            }
            "add_preview" => {
                state.add_preview(
                    input["value"].as_u64().unwrap() as u32,
                    n("offset"),
                    n("total_distance"),
                );
                serde_json::Value::Null
            }
            "classify" => serde_json::json!(state.classify(
                n("speed"),
                n("total_distance"),
                b("starting"),
                input["start_distance"].as_f64()
            )),
            "update" => serde_json::json!(state.update(
                b("warning_active"),
                n("warning_speed"),
                n("total_distance"),
                b("accel_rising"),
                b("mobile_zone_decel"),
                b("skip_box"),
                b("skip_mobile_zone"),
                b("skip_unknown")
            )),
            other => panic!("unknown operation {other}"),
        };
        compare(&result, &row["result"], &format!("result/{i}"));
        compare(
            &serde_json::to_value(state).unwrap(),
            &row["state"],
            &format!("state/{i}"),
        );
    }
    assert_eq!(fixture["rows"].as_array().unwrap().len(), 232);
}

#[test]
fn pinned_camera_profile_kind_speed_flag_and_offset_boundaries_match_native() {
    let data = zstd::stream::decode_all(
        include_bytes!("fixtures/speedcam-profile-final.json.zst").as_slice(),
    )
    .unwrap();
    let fixture: serde_json::Value = serde_json::from_slice(&data).unwrap();
    for (i, row) in fixture["rows"].as_array().unwrap().iter().enumerate() {
        let result = speedcam::decode_camera_profile(
            row[0].as_u64().unwrap() as u32,
            row[1].as_f64().unwrap(),
        );
        assert_eq!(serde_json::json!(result), row[2], "profile/{i}");
    }
    assert_eq!(fixture["rows"].as_array().unwrap().len(), 14343);
}
