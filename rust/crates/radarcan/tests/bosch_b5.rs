use openpilot_radarcan::decoder::hyundai::bosch::{
    birth_b5::{BirthB5, Context, Frame, Geometry, Model},
    physical::Object,
    tracking::Track,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, BufReader},
};
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
fn rows(data: &[u8]) -> impl Iterator<Item = Value> + '_ {
    BufReader::new(zstd::stream::read::Decoder::new(data).unwrap())
        .lines()
        .map(|l| serde_json::from_str(&l.unwrap()).unwrap())
}
fn decode<T: serde::de::DeserializeOwned>(v: &Value) -> T {
    serde_json::from_value(v.clone()).unwrap_or_else(|e| panic!("{e}: {v}"))
}
fn number(v: &Value) -> f64 {
    match v.as_str() {
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        Some("-Infinity") => f64::NEG_INFINITY,
        _ => v.as_f64().unwrap(),
    }
}
fn ids(v: &Value) -> BTreeSet<u32> {
    match v {
        Value::Object(map) => map.keys().map(|k| k.parse().unwrap()).collect(),
        _ => decode(v),
    }
}
fn camera(v: &Value) -> BTreeMap<u32, (i32, i64)> {
    match v {
        Value::Object(map) => map
            .iter()
            .map(|(k, v)| {
                (
                    k.parse().unwrap(),
                    (v[0].as_i64().unwrap() as i32, v[1].as_i64().unwrap()),
                )
            })
            .collect(),
        Value::Array(rows) => rows
            .iter()
            .map(|v| {
                (
                    v[0].as_u64().unwrap() as u32,
                    (v[1][0].as_i64().unwrap() as i32, v[1][1].as_i64().unwrap()),
                )
            })
            .collect(),
        _ => panic!("association input {v}"),
    }
}
#[test]
fn b5_complete_public_ingestion_birth_history_decisions_and_publication_match_source() {
    let constructors: Vec<Value> =
        serde_json::from_slice(include_bytes!("fixtures/bosch-b5-constructors.json")).unwrap();
    for (i, row) in constructors.iter().enumerate() {
        compare(
            &json!(BirthB5::new(decode(&row["mode"]))),
            &row["result"],
            &format!("constructor/{i}"),
        );
    }
    let mut counts = BTreeMap::<String, usize>::new();
    for (i, row) in rows(include_bytes!("fixtures/bosch-b5-final.jsonl.zst")).enumerate() {
        let input = &row["input"];
        let mut filter: BirthB5 = decode(&row["before"]);
        let op = row["op"].as_str().unwrap();
        *counts.entry(op.into()).or_default() += 1;
        let result: Result<Value, &str> = match op {
            "ingest_pose" => {
                filter.ingest_pose(
                    decode(&input["timestamp_ns"]),
                    if input["yaw_rate_left"].is_null() {
                        None
                    } else {
                        Some(number(&input["yaw_rate_left"]))
                    },
                );
                Ok(Value::Null)
            }
            "ingest_speed" => {
                filter.ingest_speed(decode(&input["timestamp_ns"]), number(&input["v_ego"]));
                Ok(Value::Null)
            }
            "ingest_model" => {
                let model: Option<Model> = decode(&input["model"]);
                filter.ingest_model(model.as_ref(), decode(&input["publication_ns"]));
                Ok(Value::Null)
            }
            "reset" => {
                filter.reset(input["reason"].as_str().unwrap());
                Ok(Value::Null)
            }
            "publication_view" => Ok(json!(
                filter.publication_view(&decode::<Vec<Object>>(&input["objects"]))
            )),
            "update" => {
                let objects: Vec<Object> = decode(&input["objects"]);
                let raw: Vec<Track> = decode(&input["raw_tracks"]);
                let camera = camera(&input["camera_associations"]);
                let word0 = ids(&input["word0_pids"]);
                let live_pids = ids(&input["live_pids"]);
                let live_raw = ids(&input["live_raw_ids"]);
                filter
                    .update(Frame {
                        objects: &objects,
                        raw: &raw,
                        scan: decode(&input["timestamp_ns"]),
                        // Three private provider probes omit the clock but have no
                        // newborns; the source never reads this argument there.
                        now: input["now_ns"].as_i64().unwrap_or_else(|| {
                            assert!(objects.iter().all(|o| o.age_scans != 1));
                            0
                        }),
                        ego: number(&input["v_ego"]),
                        yaw: if input["yaw_rate_left"].is_null() {
                            None
                        } else {
                            Some(number(&input["yaw_rate_left"]))
                        },
                        camera: &camera,
                        oem_state: decode(&input["oem_state"]),
                        word0: &word0,
                        live_pids: &live_pids,
                        live_raw: &live_raw,
                    })
                    .map(|v| json!(v))
            }
            _ => unreachable!(),
        };
        if let Some(error) = row.get("error") {
            assert_eq!(
                result.expect_err("source operation must fail"),
                error["message"].as_str().unwrap(),
                "b5/{i}/error"
            );
        } else {
            compare(
                &result.unwrap(),
                &row["result"],
                &format!("b5/{i}/{op}/result"),
            );
        }
        compare(&json!(filter), &row["after"], &format!("b5/{i}/{op}/state"));
    }
    assert_eq!(counts.values().sum::<usize>(), 6812);
    assert_eq!(counts["update"], 5768);
    assert_eq!(counts["ingest_model"], 3);
}
#[test]
fn b5_motion_geometry_alignment_lane_and_corridor_outputs_match_source_bits() {
    let mut count = 0;
    for (i, row) in rows(include_bytes!("fixtures/bosch-b5-math-final.jsonl.zst")).enumerate() {
        count += 1;
        let input = &row["input"];
        let mut filter = BirthB5::new(1);
        filter.speed_samples = decode(&row["before"]["speed_samples"]);
        filter.pose_samples = decode(&row["before"]["pose_samples"]);
        filter.model_contexts = decode(&row["before"]["model_contexts"]);
        let result: Result<Value, &str> = match row["op"].as_str().unwrap() {
            "_integrate_motion" => Ok(json!(
                filter.integrate_motion(decode(&input["start_ns"]), decode(&input["end_ns"]))
            )),
            "_geometry" => Ok(json!(filter.geometry(
                &decode::<Context>(&input["context"]),
                decode(&input["scan_ns"])
            ))),
            "_align" => Ok(json!(BirthB5::align(
                &decode::<Object>(&input["obj"]),
                &decode::<Geometry>(&input["geometry"])
            ))),
            "_instant_lane" => Ok(json!(BirthB5::instant_lane(
                &decode::<Object>(&input["obj"]),
                &decode::<Geometry>(&input["geometry"])
            ))),
            "_corridor_clear" => filter
                .corridor_clear(
                    &decode::<Object>(&input["obj"]),
                    decode(&input["scan_ns"]),
                    decode(&input["now_ns"]),
                )
                .map(|v| json!(v)),
            _ => unreachable!(),
        };
        if let Some(error) = row.get("error") {
            assert_eq!(
                result.expect_err("source operation must fail"),
                error["message"].as_str().unwrap(),
                "math/{i}/error"
            );
        } else {
            compare(
                &result.unwrap(),
                &row["result"],
                &format!("math/{i}/result"),
            );
        }
        compare(
            &json!({"speed_samples":filter.speed_samples,"pose_samples":filter.pose_samples,"model_contexts":filter.model_contexts}),
            &row["after"],
            &format!("math/{i}/state"),
        );
    }
    assert_eq!(count, 231);
}

#[test]
fn b5_model_ingestion_nonfinite_zip_shape_repeat_and_rollback_match_source() {
    let bytes = zstd::stream::decode_all(
        include_bytes!("fixtures/bosch-b5-ingest-final.json.zst").as_slice(),
    )
    .unwrap();
    let rows: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.len(), 256);
    for (i, row) in rows.iter().enumerate() {
        let mut filter: BirthB5 = decode(&row["before"]);
        let input = &row["input"];
        let model: Model = decode(&input["model"]);
        filter.ingest_model(Some(&model), decode(&input["publication_ns"]));
        compare(&json!(filter), &row["after"], &format!("ingest/{i}/state"));
    }
}
