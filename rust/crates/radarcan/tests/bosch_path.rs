use openpilot_radarcan::decoder::hyundai::bosch::{
    birth_b5::Number, unsupported_path::birth_d_path,
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
fn unsupported_birth_polyline_projection_matches_original_source() {
    let bytes = zstd::stream::decode_all(
        include_bytes!("fixtures/bosch-unsupported-path-final.json.zst").as_slice(),
    )
    .unwrap();
    let rows: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.len(), 2912);
    for (i, row) in rows.iter().enumerate() {
        let input = &row["input"];
        let path: Vec<(f64, f64)> = input["path"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (number(&p[0]), number(&p[1])))
            .collect();
        let value = birth_d_path(&path, number(&input["x"]), number(&input["y"]));
        if let Some(error) = row.get("error") {
            assert_eq!(value.unwrap_err(), error["message"].as_str().unwrap());
        } else {
            compare(
                &json!(value.unwrap().map(Number)),
                &row["result"],
                &format!("path/{i}"),
            );
        }
    }
}
