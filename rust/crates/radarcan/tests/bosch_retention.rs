use openpilot_radarcan::decoder::hyundai::bosch::{
    group_config::Config,
    physical::Object,
    retention::{Frame, Retention},
    tracking::Track,
};
use serde_json::Value;
use std::collections::BTreeMap;
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
#[test]
fn mature_retention_complete_certificates_surface_guards_archives_and_stats_match_source() {
    let bytes = zstd::stream::decode_all(
        include_bytes!("fixtures/bosch-retention-final.json.zst").as_slice(),
    )
    .unwrap();
    let rows: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.len(), 1090);
    for (i, row) in rows.iter().enumerate() {
        let input = &row["input"];
        assert_eq!(input["oem_nearer_active"].as_bool(), Some(true));
        let mut retention: Retention = serde_json::from_value(row["before"].clone()).unwrap();
        let config: Config = serde_json::from_value(input["config"].clone()).unwrap();
        let raw: Vec<Track> = serde_json::from_value(input["raw"].clone()).unwrap();
        let previous: Vec<Object> = serde_json::from_value(input["previous"].clone()).unwrap();
        let index: BTreeMap<u32, usize> =
            serde_json::from_value(input["raw_index"].clone()).unwrap();
        let mut compatible: Vec<u32> = serde_json::from_value(input["compatible"].clone()).unwrap();
        let mut cost: Vec<f64> = serde_json::from_value(input["pair_cost"].clone()).unwrap();
        let previous_refs: Vec<_> = previous.iter().collect();
        let frame = Frame {
            config: &config,
            timestamp: input["timestamp"].as_i64().unwrap(),
            raw: &raw,
            previous: &previous_refs,
            index: &index,
            ego: input["ego"].as_f64().unwrap_or(f64::NAN),
        };
        retention
            .before(frame, &mut compatible, &mut cost)
            .unwrap_or_else(|e| panic!("case/{i}: {e}"));
        compare(
            &serde_json::to_value(retention).unwrap(),
            &row["after"],
            &format!("state/{i}"),
        );
        compare(
            &serde_json::to_value(compatible).unwrap(),
            &row["compatible"],
            &format!("compatible/{i}"),
        );
        compare(
            &serde_json::to_value(cost).unwrap(),
            &row["pair_cost"],
            &format!("cost/{i}"),
        );
    }
}
