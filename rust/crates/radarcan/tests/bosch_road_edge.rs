use openpilot_radarcan::decoder::hyundai::bosch::{
    birth_b5::Model,
    road_edge::{GeometryKey, Ingest, RoadEdge, ScanObject, Validated},
};
use serde_json::{json, Value};
use std::sync::Arc;
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

fn key(v: &Value) -> GeometryKey {
    v.as_array()
        .unwrap()
        .iter()
        .map(|line| {
            fn axis(v: &Value) -> Vec<u64> {
                let s = v["source_bytes_hex"].as_str().unwrap().as_bytes();
                let bytes: Vec<u8> = s
                    .chunks_exact(2)
                    .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
                    .collect();
                bytes
                    .chunks_exact(8)
                    .map(|p| u64::from_le_bytes(p.try_into().unwrap()))
                    .collect()
            }
            (axis(&line[0]), axis(&line[1]))
        })
        .collect()
}
fn key_json(v: &GeometryKey) -> Value {
    fn axis(v: &[u64]) -> Value {
        let s: String = v
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .map(|v| format!("{v:02x}"))
            .collect();
        json!({"source_bytes_hex":s})
    }
    json!(v
        .iter()
        .map(|(x, y)| json!([axis(x), axis(y)]))
        .collect::<Vec<_>>())
}
fn restore(v: &Value) -> RoadEdge {
    let mut s = RoadEdge::new(decode(&v["enabled"]));
    s.model_edges = decode(&v["model_edges"]);
    s.publication_model_edges = decode(&v["publication_model_edges"]);
    s.last_ns = decode(&v["last_ns"]);
    s.source_model_ns = decode(&v["source_model_ns"]);
    s.would_suppress = decode(&v["would_suppress"]);
    s.publication_suppressed = decode(&v["publication_suppressed"]);
    if !v["_validated_lines"].is_null() {
        s.validated = Some(Validated {
            key: key(&v["_validated_lines"][0]),
            lines: decode(&v["_validated_lines"][1]),
        });
    }
    if !v["_last_ingest"].is_null() {
        let cache = &v["_last_ingest"];
        s.last_ingest = Some(Ingest {
            model: decode::<Option<Model>>(&cache[0]).map(Arc::new),
            timestamp: decode(&cache[1]),
            invalid: decode(&cache[2]),
        });
    }
    s
}
fn compare_state(s: &RoadEdge, v: &Value, path: &str) {
    let mut expected = v.clone();
    expected.as_object_mut().unwrap().remove("_validated_lines");
    expected.as_object_mut().unwrap().remove("_last_ingest");
    compare(
        &json!({"enabled":s.enabled,"model_edges":s.model_edges,"publication_model_edges":s.publication_model_edges,"last_ns":s.last_ns,"source_model_ns":s.source_model_ns,"would_suppress":s.would_suppress,"publication_suppressed":s.publication_suppressed}),
        &expected,
        path,
    );
    let validated = s
        .validated
        .as_ref()
        .map(|v| json!([key_json(&v.key), v.lines]));
    compare(
        &json!(validated),
        &v["_validated_lines"],
        &format!("{path}/validated"),
    );
    if let Some(ingest) = &s.last_ingest {
        let expected = &v["_last_ingest"];
        assert!(!expected.is_null(), "{path}/last-ingest");
        assert_eq!(ingest.timestamp, expected[1].as_i64().unwrap());
        assert_eq!(ingest.invalid, expected[2].as_bool().unwrap());
        let actual = ingest.model.as_ref().map(|m| json!([m.edges, m.edge_stds]));
        let expected: Option<Model> = decode(&expected[0]);
        let expected = expected.map(|m| json!([m.edges, m.edge_stds]));
        compare(
            &json!(actual),
            &json!(expected),
            &format!("{path}/model-input"),
        );
    } else {
        assert!(v["_last_ingest"].is_null());
    }
}
#[test]
fn road_edge_model_cache_ingest_invalidation_scan_and_publication_match_source() {
    let mut count = 0;
    for (i, row) in rows(include_bytes!("fixtures/bosch-road-edge-final.jsonl.zst")).enumerate() {
        let mut state = restore(&row["before"]);
        let input = &row["input"];
        let value = match row["op"].as_str().unwrap() {
            "reset" => {
                state.reset();
                Value::Null
            }
            "invalidate_model" => {
                state.invalidate_model();
                Value::Null
            }
            "ingest_model" => {
                let model = if input["model"].is_null() {
                    None
                } else if input["model"]["source_slots_type"] == "BoschModelSnapshot"
                    && row["before"]["_last_ingest"][0] == input["model"]
                {
                    state.last_ingest.as_ref().unwrap().model.clone()
                } else {
                    Some(Arc::new(decode(&input["model"])))
                };
                state.ingest_model(model, decode(&input["model_ns"]));
                Value::Null
            }
            "update" => {
                let objects: Vec<_> = input["objects"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|o| ScanObject {
                        id: decode(&o["physical_track_id"]),
                        timestamp: decode(&o["timestamp_ns"]),
                        d: number(&o["d_rel"]),
                        y: number(&o["y_rel"]),
                        oem: decode(&o["oem_selected"]),
                        vision: decode(&o["vision_supported"]),
                    })
                    .collect();
                state.update(
                    &objects,
                    decode(&input["timestamp_ns"]),
                    decode(&input["valid"]),
                    &camera(&input["camera_associations"]),
                    &ids(&input["word0_pids"]),
                );
                Value::Null
            }
            "publication_view" => json!(state.publication_by(
                &decode::<Vec<Value>>(&input["objects"]),
                decode(&input["timestamp_ns"]),
                |o| (decode(&o["physical_track_id"]), decode(&o["timestamp_ns"]))
            )),
            _ => panic!("unknown op"),
        };
        compare(&value, &row["result"], &format!("operation/{i}"));
        compare_state(&state, &row["after"], &format!("state/{i}"));
        count += 1;
    }
    assert_eq!(count, 921);
}

#[derive(serde::Deserialize)]
struct OwnedSnapshot {
    objects: Vec<openpilot_radarcan::decoder::hyundai::bosch::camera::CameraObject>,
    count: usize,
    complete: i64,
}
#[derive(serde::Deserialize)]
struct ProviderInput {
    enabled: bool,
    can_error: bool,
    wrong_config: bool,
    publication: Option<i64>,
    scan: Option<i64>,
    context: Option<openpilot_radarcan::decoder::hyundai::bosch::unsupported_birth::Context>,
    identity: Option<(i64, usize, bool)>,
    extended_ns: Option<i64>,
    association: Option<(i32, i64, i64)>,
    processed: bool,
    snapshot: Option<OwnedSnapshot>,
}
fn physical_object(v: &Value) -> openpilot_radarcan::decoder::hyundai::bosch::physical::Object {
    use openpilot_radarcan::decoder::hyundai::bosch::physical::{Object, Surface};
    Object {
        physical_track_id: decode(&v["physical_track_id"]),
        timestamp_ns: decode(&v["timestamp_ns"]),
        members: decode(&v["members"]),
        representative_raw_track_id: decode(&v["representative_raw_track_id"]),
        d_rel: number(&v["d_rel"]),
        y_rel: number(&v["y_rel"]),
        v_rel: number(&v["v_rel"]),
        oem_selected: decode(&v["oem_selected"]),
        vision_supported: decode(&v["vision_supported"]),
        age_scans: decode(&v["age_scans"]),
        grouping_evidence: decode(&v["grouping_evidence"]),
        published_surface: if v["published_surface"].is_null() {
            None
        } else {
            let s = &v["published_surface"];
            Some(Surface {
                raw_track_id: decode(&s["raw_track_id"]),
                d_rel: number(&s["d_rel"]),
                y_rel: number(&s["y_rel"]),
                v_rel: number(&s["v_rel"]),
                lateral_estimate: decode(&s["lateral_estimate"]),
            })
        },
    }
}
#[test]
fn road_edge_final_float32_surface_recheck_uses_real_unsupported_birth_verdict() {
    use indexmap::IndexMap;
    use openpilot_radarcan::decoder::hyundai::bosch::unsupported_birth::{
        Frame, Snapshot, UnsupportedBirth,
    };
    let bytes = zstd::stream::decode_all(
        include_bytes!("fixtures/bosch-road-surface-final.json.zst").as_slice(),
    )
    .unwrap();
    let rows: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.len(), 894);
    let mut excluded = 0;
    let mut errors = 0;
    for (i, row) in rows.iter().enumerate() {
        let input = &row["input"];
        let provider: ProviderInput = decode(&input["frame"]);
        let identity = provider.identity.into_iter().map(|v| (1, v)).collect();
        let associations: IndexMap<_, _> =
            provider.association.into_iter().map(|v| (1, v)).collect();
        let processed = if provider.processed {
            BTreeSet::from([1])
        } else {
            BTreeSet::new()
        };
        let frame = Frame {
            can_error: provider.can_error,
            wrong_config: provider.wrong_config,
            context: provider.context.as_ref(),
            scan: provider.scan,
            identity: &identity,
            extended_ns: provider.extended_ns,
            associations: &associations,
            processed: &processed,
            snapshot: provider.snapshot.as_ref().map(|s| Snapshot {
                objects: &s.objects,
                count: s.count,
                complete: s.complete,
            }),
        };
        let unsupported = UnsupportedBirth::new(provider.enabled);
        let mut state = restore(&row["before"]);
        let object = physical_object(&input["object"]);
        let result =
            state.publication_surface_view(&frame, &unsupported, &[object], provider.publication);
        if let Some(error) = row.get("error") {
            match result {
                Err(message) => {
                    assert_eq!(message, error["message"].as_str().unwrap(), "surface/{i}")
                }
                Ok(_) => panic!("surface/{i} source failed"),
            };
            errors += 1;
        } else {
            let ids: Vec<_> = result
                .unwrap()
                .iter()
                .map(|o| o.physical_track_id)
                .collect();
            excluded += usize::from(ids.is_empty());
            compare(&json!(ids), &row["result_ids"], &format!("surface/{i}"));
        }
        compare_state(&state, &row["after"], &format!("surface-state/{i}"));
    }
    assert_eq!(excluded, 113);
    assert_eq!(errors, 2);
}
