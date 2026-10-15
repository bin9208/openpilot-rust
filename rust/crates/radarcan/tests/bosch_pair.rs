use openpilot_radarcan::decoder::hyundai::bosch::{
    group_config::Config,
    group_pairs::{self, Bundle, Context, Evidence, Pair},
    tracking::Track,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
fn decode<T: DeserializeOwned>(value: &Value) -> BTreeMap<Pair, T> {
    value
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| {
            let parts: Vec<u32> = key
                .trim_matches(['(', ')'])
                .split(',')
                .map(|s| s.trim().parse().unwrap())
                .collect();
            (
                (parts[0], parts[1]),
                serde_json::from_value(value.clone()).unwrap(),
            )
        })
        .collect()
}
fn encode<T: Serialize>(map: &BTreeMap<Pair, T>) -> Value {
    Value::Object(
        map.iter()
            .map(|(&(a, b), v)| (format!("({a}, {b})"), serde_json::to_value(v).unwrap()))
            .collect(),
    )
}
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
#[test]
fn pair_evidence_provisional_geometry_and_rejection_counters_match_actual_source_states() {
    let bytes =
        zstd::stream::decode_all(include_bytes!("fixtures/bosch-pair-final.json.zst").as_slice())
            .unwrap();
    let rows: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.len(), 1297);
    for (i, row) in rows.iter().enumerate() {
        let input = &row["input"];
        let config: Config = serde_json::from_value(input["config"].clone()).unwrap();
        let raw: Vec<Track> = serde_json::from_value(input["raw"].clone()).unwrap();
        let mut pairs: BTreeMap<Pair, Evidence> = decode(&input["pairs"]);
        let bundles: BTreeMap<Pair, Bundle> = decode(&input["bundles"]);
        let context = Context {
            timestamp: input["timestamp"].as_i64().unwrap(),
            yaw: input["yaw"].as_f64(),
            ego: input["ego"].as_f64().unwrap_or(f64::NAN),
            oem: input["oem"].as_u64().map(|v| v as usize),
        };
        let out = group_pairs::update(&context, &raw, &config, &mut pairs, &bundles);
        let actual = json!({"pairs":encode(&pairs),"compatible":out.compatible,"pair_cost":out.pair_cost,"provisional_matches":encode(&out.provisional_matches),"provisional_handoffs":out.provisional_handoffs,"pair_candidates":out.pair_candidates,"lateral_rejections":out.lateral_rejections,"velocity_rejections":out.velocity_rejections,"motion_rejections":out.motion_rejections});
        compare(&actual, &row["result"], &format!("pairs/{i}"));
    }
}
