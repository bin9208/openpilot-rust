use openpilot_radarcan::decoder::hyundai::bosch::{acceleration::Estimator, physical::Object};
use serde_json::{json, Value};
use std::collections::BTreeSet;
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
                a.keys().collect::<std::collections::BTreeSet<_>>(),
                b.keys().collect::<std::collections::BTreeSet<_>>(),
                "{path}"
            );
            for (k, a) in a {
                compare(a, &b[k], &format!("{path}/{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
pub(crate) fn check(rows: impl Iterator<Item = Value>) {
    let mut estimator = Estimator::new();
    let mut count = 0;
    for (i, row) in rows.enumerate() {
        match row["op"].as_str().unwrap() {
            "new" => estimator = Estimator::new(),
            "reset" => estimator.reset(row["reason"].as_str().unwrap()),
            "update" => {
                count += 1;
                let objects: Vec<Object> = serde_json::from_value(row["objects"].clone()).unwrap();
                let live: BTreeSet<u32> = serde_json::from_value(row["live"].clone()).unwrap();
                let result = estimator
                    .update(
                        &objects,
                        row["scan"].as_i64(),
                        row["ego"].as_f64().unwrap_or(f64::NAN),
                        &live,
                    )
                    .unwrap();
                let result: serde_json::Map<String, Value> = result
                    .iter()
                    .map(|(&pid, &v)| {
                        (
                            pid.to_string(),
                            if v.is_nan() { json!("NaN") } else { json!(v) },
                        )
                    })
                    .collect();
                compare(
                    &Value::Object(result),
                    &row["result"],
                    &format!("result/{i}"),
                );
            }
            _ => panic!("unknown source operation"),
        }
        if let Some(debug) = row.get("debug") {
            compare(
                &estimator.debug_snapshot(row["debug_now"].as_i64().unwrap()),
                debug,
                &format!("debug/{i}"),
            );
        }
        compare(
            &serde_json::to_value(&estimator).unwrap(),
            &row["state"],
            &format!("state/{i}"),
        );
    }
    assert_eq!(count, 2544);
}

pub(crate) fn frozen_fixture() -> impl std::io::Read {
    zstd::stream::read::Decoder::new(
        include_bytes!("../fixtures/bosch-acceleration-final.jsonl.zst").as_slice(),
    )
    .unwrap()
}
