use openpilot_radarcan::decoder::hyundai::bosch::{physical::Object, sidepass::SidePass};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
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
fn sidepass_lateral_slide_arm_release_realign_and_surface_match_source() {
    let constructors: Vec<Value> = serde_json::from_slice(include_bytes!(
        "fixtures/bosch-lateral-publication-constructors.json"
    ))
    .unwrap();
    for (i, row) in constructors.iter().enumerate() {
        use openpilot_radarcan::decoder::hyundai::bosch::persistent_clone::PersistentClone;
        let mode = row["mode"].as_u64().unwrap() as u8;
        let result: Result<Value, &str> = match row["name"].as_str().unwrap() {
            "_BoschSidePassLateralEstimator" => SidePass::new(mode).map(|v| json!(v)),
            "_BoschPersistentSpatialCloneFilter" => PersistentClone::new(mode).map(|v| json!(v)),
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
    for (i, row) in rows(include_bytes!("fixtures/bosch-sidepass-final.jsonl.zst")).enumerate() {
        count += 1;
        let input = &row["input"];
        let mut estimator: SidePass = decode(&row["before"]);
        let result: Result<Value, &str> = match row["op"].as_str().unwrap() {
            "reset" => {
                estimator.reset(
                    decode(&input["timestamp_ns"]),
                    input["reason"].as_str().unwrap(),
                );
                Ok(Value::Null)
            }
            "publication_view" => Ok(json!(
                estimator.publication_view(&decode::<Vec<Object>>(&input["objects"]))
            )),
            "update" => estimator
                .update(
                    &decode::<Vec<Object>>(&input["objects"]),
                    decode(&input["timestamp_ns"]),
                    input["v_ego"].as_f64().unwrap_or(f64::NAN),
                    &decode::<Vec<(f64, f64)>>(&input["path"]),
                    &decode(&input["oem_pids"]),
                )
                .map(|v| json!(v)),
            _ => unreachable!(),
        };
        if let Some(error) = row.get("error") {
            assert_eq!(
                result.expect_err("source operation must fail"),
                error["message"].as_str().unwrap()
            );
        } else {
            compare(
                &result.unwrap(),
                &row["result"],
                &format!("sidepass/{i}/result"),
            );
        }
        compare(
            &json!(estimator),
            &row["after"],
            &format!("sidepass/{i}/state"),
        );
    }
    assert_eq!(count, 1184);
}
#[test]
fn persistent_spatial_copy_support_welford_continuity_and_proof_match_source() {
    use openpilot_radarcan::decoder::hyundai::bosch::persistent_clone::PersistentClone;
    let mut count = 0;
    for (i, row) in rows(include_bytes!(
        "fixtures/bosch-persistent-clone-final.jsonl.zst"
    ))
    .enumerate()
    {
        count += 1;
        let input = &row["input"];
        let mut filter: PersistentClone = decode(&row["before"]);
        assert_eq!(row["op"], "update");
        let result = filter.update(
            &decode::<Vec<Object>>(&input["objects"]),
            decode(&input["timestamp_ns"]),
            input["v_ego"].as_f64().unwrap_or(f64::NAN),
            &decode(&input["word0_pids"]),
            input["yaw_rate"].as_f64(),
        );
        compare(&json!(result), &row["result"], &format!("p91/{i}/result"));
        compare(&json!(filter), &row["after"], &format!("p91/{i}/state"));
    }
    assert_eq!(count, 350);
}
