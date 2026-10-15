use openpilot_radarcan::decoder::hyundai::bosch::{
    mirror_birth::{round_one, Frame, MirrorBirth},
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

fn operation(row: &Value, index: &str) {
    let input = &row["input"];
    let mut mirror: MirrorBirth = decode(&row["before"]);
    let result: Result<Value, &str> = match row["op"].as_str().unwrap() {
        "reset" => {
            mirror.reset(
                decode(&input["timestamp_ns"]),
                input["reason"].as_str().unwrap(),
            );
            Ok(Value::Null)
        }
        "publication_view" => Ok(json!(
            mirror.publication_by(&decode::<Vec<Value>>(&input["objects"]), |o| decode(&o["physical_track_id"]))
        )),
        "update" => {
            let objects: Vec<Object> = decode(&input["objects"]);
            let raw: Vec<Track> = decode(&input["raw_tracks"]);
            let camera = camera(&input["camera_associations"]);
            let word0 = ids(&input["word0_pids"]);
            let live = if input["live_pids"].is_null() {
                None
            } else {
                Some(ids(&input["live_pids"]))
            };
            mirror
                .update(Frame {
                    objects: &objects,
                    raw: &raw,
                    timestamp: decode(&input["timestamp_ns"]),
                    ego: number(&input["v_ego"]),
                    yaw: if input["yaw_rate_left"].is_null() {
                        None
                    } else {
                        Some(number(&input["yaw_rate_left"]))
                    },
                    camera: &camera,
                    word0: &word0,
                    live: live.as_ref(),
                })
                .map(|v| json!(v))
        }
        _ => unreachable!(),
    };
    if let Some(error) = row.get("error") {
        assert_eq!(
            result.expect_err("source operation must fail"),
            error["message"].as_str().unwrap(),
            "{index}"
        );
    } else {
        compare(&result.unwrap(), &row["result"], &format!("{index}/result"));
    }
    compare(&json!(mirror), &row["after"], &format!("{index}/state"));
}
#[test]
fn mirror_wall_proof_histories_holds_identity_budget_and_publication_match_source() {
    let constructors: Vec<Value> =
        serde_json::from_slice(include_bytes!("fixtures/bosch-mirror-constructors.json")).unwrap();
    for (i, row) in constructors.iter().enumerate() {
        let result = MirrorBirth::new(decode(&row["mode"])).map(|v| json!(v));
        if let Some(error) = row.get("error") {
            assert_eq!(
                result.expect_err("source constructor must fail"),
                error["message"].as_str().unwrap()
            );
        } else {
            compare(
                &result.unwrap(),
                &row["result"],
                &format!("constructor/{i}"),
            );
        }
    }
    let mut count = 0;
    for (i, row) in rows(include_bytes!(
        "fixtures/bosch-mirror-birth-final.jsonl.zst"
    ))
    .enumerate()
    {
        count += 1;
        operation(&row, &format!("mirror/{i}"));
    }
    assert_eq!(count, 2693);
}
#[test]
fn mirror_nonfinite_motion_errors_and_reset_states_match_source() {
    let bytes = zstd::stream::decode_all(
        include_bytes!("fixtures/bosch-mirror-extra-final.json.zst").as_slice(),
    )
    .unwrap();
    let rows: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.len(), 50);
    for (i, row) in rows.iter().enumerate() {
        operation(row, &format!("extra/{i}"));
    }
}
#[test]
fn decimal_round_one_matches_cpython_all_ieee_classes_and_halfway_neighbors() {
    let bytes = zstd::stream::decode_all(
        include_bytes!("fixtures/bosch-round-one-final.json.zst").as_slice(),
    )
    .unwrap();
    let rows: Vec<[u64; 2]> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.len(), 32775);
    for (i, [input, expected]) in rows.into_iter().enumerate() {
        assert_eq!(
            round_one(f64::from_bits(input)).to_bits(),
            expected,
            "round/{i}/input={input:016x}"
        );
    }
}
