use openpilot_radarcan::decoder::hyundai::bosch::{
    ancestry::Ancestry,
    group::Manager,
    physical::{Object, VisionCue},
    tracking::Track,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, BufReader},
};
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
fn rows(data: &[u8]) -> impl Iterator<Item = Value> + '_ {
    BufReader::new(zstd::stream::read::Decoder::new(data).unwrap())
        .lines()
        .map(|l| serde_json::from_str(&l.unwrap()).unwrap())
}
fn decode<T: serde::de::DeserializeOwned>(v: &Value) -> T {
    serde_json::from_value(v.clone()).unwrap_or_else(|e| panic!("{e}: {v}"))
}
#[test]
fn full_group_identity_provisional_publication_lineage_and_trace_match_source() {
    let mut counts = BTreeMap::<String, usize>::new();
    for (i, row) in rows(include_bytes!("fixtures/bosch-group-final.jsonl.zst")).enumerate() {
        let input = &row["input"];
        let op = row["op"].as_str().unwrap();
        *counts.entry(op.into()).or_default() += 1;
        // Two source query probes deliberately replace live states with None.
        // The query only reads lineage, so compare that independent typed input.
        if row["before"]["states"]
            .as_object()
            .unwrap()
            .values()
            .any(Value::is_null)
        {
            assert_eq!(op, "common_ancestry_evidence");
            let ancestry: Ancestry = decode(&row["before"]);
            compare(
                &json!(ancestry.evidence(
                    &decode(&input["first"]),
                    &decode(&input["second"]),
                    decode(&input["shared_start_ns"]),
                    decode(&input["timestamp_ns"])
                )),
                &row["result"],
                &format!("case/{i}/query"),
            );
            for field in ["common_group_ancestry", "common_representative_owners"] {
                compare(
                    &json!(ancestry)[field],
                    &row["after"][field],
                    &format!("case/{i}/{field}"),
                );
            }
            continue;
        }
        let mut manager: Manager = decode(&row["before"]);
        let result: Result<Value, &str> = match op {
            "update" => {
                let raw: Vec<Track> = decode(&input["raw_tracks"]);
                let vision: Vec<VisionCue> = decode(&input["vision"]);
                manager
                    .update(
                        decode(&input["timestamp_ns"]),
                        &raw,
                        decode(&input["yaw_rate"]),
                        input["v_ego"].as_f64().unwrap_or(f64::NAN),
                        decode(&input["oem_slot"]),
                        &vision,
                    )
                    .map(|r| json!(r))
            }
            "common_ancestry_evidence" => Ok(json!(manager.ancestry.evidence(
                &decode(&input["first"]),
                &decode(&input["second"]),
                decode(&input["shared_start_ns"]),
                decode(&input["timestamp_ns"])
            ))),
            "reset_common_ancestry" => {
                manager.ancestry.reset();
                Ok(Value::Null)
            }
            "reset_provisional" => {
                manager.reset_provisional(
                    decode(&input["timestamp_ns"]),
                    input["reason"].as_str().unwrap(),
                );
                Ok(Value::Null)
            }
            "provisional_publication_view" => {
                let objects: Vec<Object> = decode(&input["objects"]);
                let strict = if input["strict_associations"].is_null() {
                    BTreeMap::new()
                } else {
                    decode(&input["strict_associations"])
                };
                let oem = decode(&input["oem_pids"]);
                Ok(json!(
                    manager.provisional_publication_view(&objects, &strict, &oem)
                ))
            }
            _ => panic!("unknown operation {op}"),
        };
        if let Some(error) = row.get("error") {
            assert_eq!(
                match result {
                    Ok(_) => panic!("source operation must fail"),
                    Err(e) => e,
                },
                error["message"].as_str().unwrap(),
                "case/{i}/{op}"
            );
        } else {
            compare(
                &result.unwrap(),
                &row["result"],
                &format!("case/{i}/{op}/result"),
            );
        }
        compare(
            &json!(manager),
            &row["after"],
            &format!("case/{i}/{op}/state"),
        );
        if let Some(expected) = row.get("diagnostics") {
            compare(
                &json!(manager.diagnostics()),
                expected,
                &format!("case/{i}/{op}/diagnostics"),
            );
        }
    }
    assert_eq!(counts.values().sum::<usize>(), 1439);
    assert_eq!(counts["update"], 1298);
    assert_eq!(counts["common_ancestry_evidence"], 117);
}
#[test]
fn bounded_ancestry_all_mutations_and_queries_match_original_source() {
    let mut count = 0;
    for (i, row) in rows(include_bytes!("fixtures/bosch-ancestry-final.jsonl.zst")).enumerate() {
        count += 1;
        let input = &row["input"];
        let mut ancestry: Ancestry = decode(&row["before"]);
        let result = match row["op"].as_str().unwrap() {
            "_update_common_ancestry" => {
                let states: BTreeMap<u32, BTreeSet<u32>> = decode(&input["states"]);
                ancestry.update(
                    decode(&input["timestamp_ns"]),
                    &states,
                    &decode::<Vec<Object>>(&input["objects"]),
                );
                Value::Null
            }
            "common_ancestry_evidence" => json!(ancestry.evidence(
                &decode(&input["first"]),
                &decode(&input["second"]),
                decode(&input["shared_start_ns"]),
                decode(&input["timestamp_ns"])
            )),
            "reset_common_ancestry" => {
                ancestry.reset();
                Value::Null
            }
            _ => unreachable!(),
        };
        compare(&result, &row["result"], &format!("ancestry/{i}/result"));
        compare(
            &json!(ancestry),
            &row["after"],
            &format!("ancestry/{i}/state"),
        );
    }
    assert_eq!(count, 1539);
}
#[test]
fn ordered_raw_and_physical_tracker_states_match_original_source() {
    use openpilot_radarcan::decoder::hyundai::bosch::{raw::RawDetection, tracker::Tracker};
    let mut count = 0;
    for (i, row) in rows(include_bytes!("fixtures/bosch-tracker-final.jsonl.zst")).enumerate() {
        count += 1;
        let input = &row["input"];
        let mut tracker: Tracker = decode(&row["before"]);
        let detections: Vec<RawDetection> = decode(&input["detections"]);
        let vision: Vec<VisionCue> = decode(&input["vision"]);
        let result = tracker.update(
            decode(&input["timestamp_ns"]),
            &detections,
            decode(&input["yaw_rate"]),
            input["v_ego"].as_f64().unwrap_or(f64::NAN),
            decode(&input["oem_slot"]),
            &vision,
        );
        if let Some(error) = row.get("error") {
            assert_eq!(
                match result {
                    Ok(_) => panic!("source operation must fail"),
                    Err(e) => e,
                },
                error["message"].as_str().unwrap(),
                "tracker/{i}/error"
            );
        } else {
            compare(
                &json!(result.unwrap()),
                &row["result"],
                &format!("tracker/{i}/result"),
            );
        }
        compare(
            &json!(tracker),
            &row["after"],
            &format!("tracker/{i}/state"),
        );
    }
    assert_eq!(count, 89);
}
