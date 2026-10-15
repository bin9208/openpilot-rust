use openpilot_radarcan::decoder::hyundai::bosch::{
    family::{Family, Frame},
    physical::Object,
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

#[test]
fn family_two_scan_proof_release_identity_and_publication_match_source() {
    let constructors: Vec<Value> = serde_json::from_slice(include_bytes!(
        "fixtures/bosch-publication-constructors.json"
    ))
    .unwrap();
    for (i, row) in constructors.iter().enumerate() {
        use openpilot_radarcan::decoder::hyundai::bosch::{
            burst::Burst,
            oem::{Gate, Intent},
        };
        let mode = row["mode"].as_u64().unwrap_or(0) as u8;
        let result: Result<Value, &str> = match row["name"].as_str().unwrap() {
            "_BoschFamilyCompanionFilter" => Family::new(mode).map(|v| json!(v)),
            "_BoschBurstMultiReturnDefer" => Burst::new(mode).map(|v| json!(v)),
            "_BoschOemValidationGate" => Gate::new(mode).map(|v| json!(v)),
            "_BoschOemIntentTracker" => Ok(json!(Intent::default())),
            "classify" => Ok(json!(Gate::classify(
                decode(&row["word0"]),
                decode(&row["word1"])
            ))),
            _ => unreachable!(),
        };
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
    for (i, row) in rows(include_bytes!("fixtures/bosch-family-final.jsonl.zst")).enumerate() {
        count += 1;
        let input = &row["input"];
        let mut family: Family = decode(&row["before"]);
        let result = match row["op"].as_str().unwrap() {
            "reset" => {
                family.reset(
                    decode(&input["timestamp_ns"]),
                    input["reason"].as_str().unwrap(),
                );
                Value::Null
            }
            "publication_view" => {
                json!(family.publication_view(&decode::<Vec<Object>>(&input["objects"])))
            }
            "update" => {
                let objects: Vec<Object> = decode(&input["objects"]);
                let strict = if input["strict_associations"].is_null() {
                    BTreeMap::new()
                } else {
                    decode(&input["strict_associations"])
                };
                let oem: BTreeSet<u32> = decode(&input["oem_pids"]);
                let excluded = decode(&input["excluded_pids"]);
                let path: Vec<(f64, f64)> = decode(&input["path"]);
                json!(family.update(Frame {
                    objects: &objects,
                    timestamp: decode(&input["timestamp_ns"]),
                    ego: input["v_ego"].as_f64().unwrap_or(f64::NAN),
                    yaw: input["yaw_rate"].as_f64(),
                    strict: &strict,
                    oem: &oem,
                    excluded: &excluded,
                    path: &path
                }))
            }
            _ => unreachable!(),
        };
        compare(&result, &row["result"], &format!("family/{i}/result"));
        compare(&json!(family), &row["after"], &format!("family/{i}/state"));
    }
    assert_eq!(count, 692);
}
#[test]
fn burst_geometry_proof_missing_scan_budget_and_publication_match_source() {
    use openpilot_radarcan::decoder::hyundai::bosch::burst::Burst;
    let mut count = 0;
    for (i, row) in rows(include_bytes!("fixtures/bosch-burst-final.jsonl.zst")).enumerate() {
        count += 1;
        let input = &row["input"];
        let mut burst: Burst = decode(&row["before"]);
        let result = match row["op"].as_str().unwrap() {
            "reset" => {
                burst.reset(
                    decode(&input["timestamp_ns"]),
                    input["reason"].as_str().unwrap(),
                );
                Value::Null
            }
            "publication_view" => {
                json!(burst.publication_view(&decode::<Vec<Object>>(&input["objects"])))
            }
            "update" => json!(burst.update(
                &decode::<Vec<Object>>(&input["objects"]),
                decode(&input["timestamp_ns"]),
                input["v_ego"].as_f64().unwrap_or(f64::NAN),
                input["yaw_rate"].as_f64(),
                &decode(&input["word0_validated_pids"])
            )),
            _ => unreachable!(),
        };
        compare(&result, &row["result"], &format!("burst/{i}/result"));
        compare(&json!(burst), &row["after"], &format!("burst/{i}/state"));
    }
    assert_eq!(count, 97);
}
#[test]
fn oem_intent_window_and_median_match_original_source() {
    use openpilot_radarcan::decoder::hyundai::bosch::oem::Intent;
    let mut count = 0;
    for (i, row) in rows(include_bytes!("fixtures/bosch-oem-intent-final.jsonl.zst")).enumerate() {
        count += 1;
        let input = &row["input"];
        let mut intent: Intent = decode(&row["before"]);
        let result = match row["op"].as_str().unwrap() {
            "ingest" => {
                let value = match input["value"].as_str() {
                    Some("NaN") => Some(f64::NAN),
                    Some("Infinity") => Some(f64::INFINITY),
                    Some("-Infinity") => Some(f64::NEG_INFINITY),
                    _ => input["value"].as_f64(),
                };
                intent.ingest(decode(&input["timestamp_ns"]), value);
                Value::Null
            }
            "median" => json!(intent.median(decode(&input["timestamp_ns"]))),
            "reset" => {
                intent.reset();
                Value::Null
            }
            _ => unreachable!(),
        };
        compare(&result, &row["result"], &format!("intent/{i}/result"));
        compare(&json!(intent), &row["after"], &format!("intent/{i}/state"));
    }
    assert_eq!(count, 837);
}
#[test]
fn oem_probation_evidence_grace_cutins_and_sibling_release_match_source() {
    use openpilot_radarcan::decoder::hyundai::bosch::oem::Gate;
    let mut count = 0;
    for (i, row) in rows(include_bytes!("fixtures/bosch-oem-gate-final.jsonl.zst")).enumerate() {
        count += 1;
        let input = &row["input"];
        let mut gate: Gate = decode(&row["before"]);
        let result = gate.update(
            &decode::<Vec<Object>>(&input["objects"]),
            decode(&input["timestamp_ns"]),
            input["v_ego"].as_f64().unwrap_or(f64::NAN),
            decode(&input["state"]),
            &decode(&input["word0_pids"]),
            decode(&input["oem_valid"]),
        );
        compare(&json!(result), &row["result"], &format!("oem/{i}/result"));
        compare(&json!(gate), &row["after"], &format!("oem/{i}/state"));
    }
    assert_eq!(count, 233);
}
