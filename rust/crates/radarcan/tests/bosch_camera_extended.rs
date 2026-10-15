use openpilot_radarcan::decoder::hyundai::bosch::{
    camera::CameraObject,
    camera_extended::{Extended, Object, Snapshot},
};
use serde_json::{json, Value};
fn compare(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            if let (Some(a), Some(b)) = (a.as_i64(), b.as_i64()) {
                assert_eq!(a, b, "{path}");
            } else if let (Some(a), Some(b)) = (a.as_u64(), b.as_u64()) {
                assert_eq!(a, b, "{path}");
            } else {
                assert_eq!(
                    a.as_f64().unwrap().to_bits(),
                    b.as_f64().unwrap().to_bits(),
                    "{path}: {a} != {b}"
                );
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}/{i}"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<Vec<_>>(),
                b.keys().collect::<Vec<_>>(),
                "{path}"
            );
            for (k, a) in a {
                compare(a, &b[k], &format!("{path}/{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
fn number(v: &Value) -> f64 {
    match v.as_str() {
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        Some("-Infinity") => f64::NEG_INFINITY,
        _ => v.as_f64().unwrap(),
    }
}

#[test]
fn complete_camera_extended_actual_source_helpers_and_state_transitions() {
    let bytes = zstd::stream::decode_all(
        include_bytes!("fixtures/bosch-camera-extended-final.json.zst").as_slice(),
    )
    .unwrap();
    let rows: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.len(), 3303);
    for (i, row) in rows.iter().enumerate() {
        assert!(row["injected_helpers"].as_array().unwrap().is_empty());
        let mut state: Extended = serde_json::from_value(row["before"].clone())
            .unwrap_or_else(|e| panic!("before/{i}: {e}"));
        let input = &row["input"];
        let objects: Vec<Object> = serde_json::from_value(input["objects"].clone())
            .unwrap_or_else(|e| panic!("objects/{i}: {e}"));
        let snapshots = row["snapshots"].as_array().unwrap();
        assert!(snapshots.len() <= 1);
        if let Some(s) = snapshots.first() {
            assert_eq!(s["arguments"][0], input["timestamp_ns"]);
        }
        let raw_snapshot = snapshots
            .first()
            .map(|s| &s["result"])
            .filter(|s| !s.is_null());
        let cameras: Vec<CameraObject> = raw_snapshot
            .map(|s| serde_json::from_value(s[0].clone()).unwrap())
            .unwrap_or_default();
        let snapshot = raw_snapshot.map(|s| Snapshot {
            objects: &cameras,
            count: s[1].as_u64().unwrap() as usize,
            complete_ns: s[3].as_i64().unwrap(),
        });
        let yaw = if input["yaw_rate"].is_null() {
            None
        } else {
            Some(number(&input["yaw_rate"]))
        };
        state
            .update(
                input["timestamp_ns"].as_i64().unwrap(),
                &objects,
                number(&input["v_ego"]),
                yaw,
                snapshot,
            )
            .unwrap_or_else(|e| panic!("update/{i}: {e}"));
        compare(&json!(state), &row["after"], &format!("camera/{i}"));
        compare(
            &row["result"],
            &input["objects"],
            &format!("identity-pass-through/{i}"),
        );
    }
}
#[test]
fn camera_extended_constructor_modes_match_source() {
    let rows: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/bosch-camera-extended-constructors.json"
    ))
    .unwrap();
    assert_eq!(rows.len(), 30);
    for (i, row) in rows.iter().enumerate() {
        let result = Extended::new(
            row["mode"].as_u64().unwrap() as u8,
            row["curve"].as_u64().unwrap() as u8,
        );
        if let Some(error) = row.get("error") {
            match result {
                Err(message) => assert_eq!(message, error["message"].as_str().unwrap()),
                Ok(_) => panic!("source constructor rejected {i}"),
            }
        } else {
            compare(
                &json!(result.unwrap()),
                &row["result"],
                &format!("constructor/{i}"),
            );
        }
    }
}
