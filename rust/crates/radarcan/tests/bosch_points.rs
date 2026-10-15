use openpilot_radarcan::{
    data::Data,
    decoder::hyundai::bosch::{physical::Object, points},
    point::Point,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
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
fn number(v: &Value) -> f64 {
    match v.as_str() {
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        Some("-Infinity") => f64::NEG_INFINITY,
        _ => v.as_f64().unwrap(),
    }
}

#[test]
fn actual_cereal_point_writers_preserve_float32_order_and_partial_errors() {
    let bytes =
        zstd::stream::decode_all(include_bytes!("fixtures/bosch-points-final.json.zst").as_slice())
            .unwrap();
    let rows: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.len(), 2048);
    for (i, row) in rows.iter().enumerate() {
        let input = &row["input"];
        let objects: Vec<Object> = serde_json::from_value(input["objects"].clone()).unwrap();
        let mut data = Data {
            points: serde_json::from_value(input["old"].clone()).unwrap(),
            ..Data::default()
        };
        let aliases: Option<BTreeMap<u32, u64>> =
            serde_json::from_value(input["alias"].clone()).unwrap();
        let accelerations: Option<BTreeMap<u32, f64>> =
            serde_json::from_value(input["a_lead"].clone()).unwrap();
        let ego = number(&input["v_ego"]);
        let result = match input["mode"].as_str().unwrap() {
            "append" => points::append(
                &mut data,
                &objects,
                ego,
                input["now_ns"].as_i64().unwrap(),
                aliases.as_ref(),
                accelerations.as_ref(),
            ),
            "native" => points::native(
                &objects,
                input["timestamp_ns"].as_i64().unwrap(),
                ego,
                input["complete"].as_bool().unwrap(),
                input["unsupported"].as_bool().unwrap(),
            )
            .map(|v| data = v),
            "make" => points::make(&objects, ego).map(|v| {
                data = Data {
                    points: v,
                    ..Data::default()
                }
            }),
            "fill" => {
                data.points = vec![Point::default(); objects.len()];
                data.points.iter_mut().zip(&objects).try_for_each(|(p, o)| {
                    points::fill(
                        p,
                        o,
                        ego,
                        aliases.as_ref(),
                        accelerations
                            .as_ref()
                            .and_then(|a| a.get(&o.physical_track_id))
                            .copied()
                            .unwrap_or(f64::NAN),
                    )
                })
            }
            mode => panic!("{mode}"),
        };
        let error = result
            .err()
            .map(|e| json!({"kind":e.kind,"message":e.message}))
            .unwrap_or(Value::Null);
        compare(&error, &row["error"], &format!("error/{i}"));
        compare(
            &json!(data.points),
            &row["after"]["points"],
            &format!("points/{i}"),
        );
        let expected=row["after"].get("errors").cloned().unwrap_or_else(||json!({"canError":false,"radarFault":false,"wrongConfig":false,"radarUnavailableTemporary":false}));
        compare(&json!(data.errors), &expected, &format!("errors/{i}"));
    }

    let bytes = zstd::stream::decode_all(
        include_bytes!("fixtures/bosch-processed-final.json.zst").as_slice(),
    )
    .unwrap();
    let rows: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.len(), 2048);
    for (i, row) in rows.iter().enumerate() {
        let objects: Vec<Object> = serde_json::from_value(row["objects"].clone()).unwrap();
        let word: Option<u32> = serde_json::from_value(row["word"].clone()).unwrap();
        compare(
            &json!(points::match_processed(&objects, word)),
            &row["result"],
            &format!("processed/{i}"),
        );
    }
}

#[test]
fn shared_model_snapshot_copies_only_source_geometry_and_first_lead() {
    use openpilot_radarcan::decoder::hyundai::bosch::model_snapshot::{Input, Snapshot};
    let bytes =
        zstd::stream::decode_all(include_bytes!("fixtures/bosch-model-final.json.zst").as_slice())
            .unwrap();
    let rows: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.len(), 1024);
    for (i, row) in rows.iter().enumerate() {
        let mut input: Input = serde_json::from_value(row["input"].clone()).unwrap();
        let snapshot = Snapshot::new(&input);
        input.geometry.edges.clear();
        input.leads.clear();
        input.geometry.position = None;
        compare(&json!(snapshot), &row["after"], &format!("model/{i}"));
        assert_eq!(
            snapshot.cues().count(),
            row["after"]["cues"].as_array().unwrap().len()
        );
        assert_eq!(
            snapshot.geometry().source_ns,
            row["after"]["timestampEof"].as_i64().unwrap()
        );
    }
}
