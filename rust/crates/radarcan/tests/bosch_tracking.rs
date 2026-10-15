use openpilot_radarcan::decoder::hyundai::bosch::{raw::RawDetection, tracking::Manager};
use serde_json::Value;
fn compare(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(
            a.as_f64().unwrap().to_bits(),
            b.as_f64().unwrap().to_bits(),
            "{path}: {a} != {b}"
        ),
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
#[test]
fn raw_track_identity_coasting_diagnostics_and_atomic_errors_match_source() {
    let bytes = zstd::stream::decode_all(
        include_bytes!("fixtures/bosch-tracking-final.json.zst").as_slice(),
    )
    .unwrap();
    let rows: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.len(), 768);
    for (i, row) in rows.iter().enumerate() {
        let mut manager: Manager = serde_json::from_value(row["before"].clone()).unwrap();
        let input = &row["input"];
        let detections: Vec<RawDetection> =
            serde_json::from_value(input["detections"].clone()).unwrap();
        let result = manager.update(
            input["timestamp_ns"].as_i64().unwrap(),
            &detections,
            input["yaw_rate"].as_f64(),
        );
        if let Some(error) = row.get("error") {
            assert_eq!(
                result.err().as_deref(),
                error["message"].as_str(),
                "error/{i}"
            );
        } else {
            let result = result.unwrap_or_else(|e| panic!("case/{i}: {e}"));
            compare(
                &serde_json::to_value(result).unwrap(),
                &row["result"],
                &format!("result/{i}"),
            );
        }
        compare(
            &serde_json::to_value(manager).unwrap(),
            &row["after"],
            &format!("state/{i}"),
        );
    }
}
