use indexmap::IndexMap;
use openpilot_radarcan::decoder::hyundai::bosch::{
    birth_b5::Model,
    mirror_m3::{round_three, BirthDecision, Frame, MirrorM3, MirrorObject},
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
        Value::Null => BTreeMap::new(),
        _ => panic!("association input {v}"),
    }
}

fn operation(row: &Value, index: &str) {
    let input = &row["input"];
    let mut state: MirrorM3 = decode(&row["before"]);
    let result = match row["op"].as_str().unwrap() {
        "reset" => {
            state.reset(
                decode(&input["timestamp_ns"]),
                input["reason"].as_str().unwrap(),
            );
            Value::Null
        }
        "ingest_model" => {
            let model: Option<Model> = decode(&input["model"]);
            state.ingest_model(model.as_ref(), decode(&input["model_ns"]));
            Value::Null
        }
        "publication_view" => json!(state
            .publication_by(&decode::<Vec<Value>>(&input["objects"]), |o| decode(
                &o["physical_track_id"]
            ))),
        "update" => {
            let mirror: IndexMap<u32, MirrorObject> = if input["mirror_objects"].is_object() {
                decode(&input["mirror_objects"])
            } else {
                IndexMap::new()
            };
            let decisions: IndexMap<u32, BirthDecision> = if input["mirror_decisions"].is_object() {
                decode(&input["mirror_decisions"])
            } else {
                IndexMap::new()
            };
            let camera = camera(&input["camera_associations"]);
            let word0 = ids(&input["word0_pids"]);
            let live = if input["live_pids"].is_null() {
                None
            } else {
                Some(ids(&input["live_pids"]))
            };
            json!(state.update(Frame {
                timestamp: decode(&input["timestamp_ns"]),
                ego: number(&input["v_ego"]),
                mirror: &mirror,
                decisions: &decisions,
                camera: &camera,
                word0: &word0,
                mirror_enabled: decode(&input["mirror_enabled"]),
                live: live.as_ref()
            }))
        }
        _ => panic!("unknown {index}"),
    };
    compare(&result, &row["result"], &format!("{index}/result"));
    compare(&json!(state), &row["after"], &format!("{index}/state"));
}
#[test]
fn m3_edge_birth_model_ingest_holds_extensions_and_publication_match_source() {
    let mut count = 0;
    for (i, row) in rows(include_bytes!("fixtures/bosch-m3-final.jsonl.zst")).enumerate() {
        operation(&row, &format!("m3/{i}"));
        count += 1;
    }
    assert_eq!(count, 2766);
}
#[test]
fn three_decimal_rounding_matches_source_ieee_and_halfway_neighbors() {
    let bytes = zstd::stream::decode_all(
        include_bytes!("fixtures/bosch-round-three-final.json.zst").as_slice(),
    )
    .unwrap();
    let rows: Vec<[u64; 2]> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.len(), 32775);
    for (i, [input, expected]) in rows.into_iter().enumerate() {
        assert_eq!(
            round_three(f64::from_bits(input)).to_bits(),
            expected,
            "round3/{i}"
        );
    }
}

#[test]
fn m3_edge_proof_accepts_source_unvalidated_model_geometry() {
    let bytes = zstd::stream::decode_all(
        include_bytes!("fixtures/bosch-m3-edge-final.json.zst").as_slice(),
    )
    .unwrap();
    let rows: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.len(), 240);
    fn object(v: &Value) -> MirrorObject {
        (
            number(&v[0]),
            number(&v[1]),
            number(&v[2]),
            decode(&v[3]),
            decode(&v[4]),
            decode(&v[5]),
            decode(&v[6]),
            decode(&v[7]),
        )
    }
    for (i, row) in rows.iter().enumerate() {
        let input = &row["input"];
        let mirror: IndexMap<u32, MirrorObject> = input["mirror"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.parse().unwrap(), object(v)))
            .collect();
        let edge = decode(&input["edge"]);
        let value = MirrorM3::edge_birth(
            decode(&input["pid"]),
            &object(&input["obj"]),
            &mirror,
            Some(&edge),
            number(&input["v_ego"]),
        );
        compare(&json!(value), &row["result"], &format!("edge/{i}"));
    }
}

#[test]
fn m3_constructor_modes_match_source() {
    let rows: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/bosch-m3-constructors.json")).unwrap();
    assert_eq!(rows.len(), 5);
    for row in rows {
        let value = MirrorM3::new(decode(&row["mode"]));
        if let Some(error) = row.get("error") {
            match value {
                Err(message) => assert_eq!(message, error["message"]),
                Ok(_) => panic!("source constructor failed"),
            }
        } else {
            compare(&json!(value.unwrap()), &row["result"], "constructor");
        }
    }
}
