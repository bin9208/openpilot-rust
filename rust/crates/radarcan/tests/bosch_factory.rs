// Reuse the complete diagnostic adapter; production decoding remains in Interface.
#[allow(dead_code)]
#[path = "../examples/radarcan_policy/base.rs"]
mod base;
#[path = "../examples/radarcan_policy/decoder.rs"]
mod decoder;
#[path = "../examples/radarcan_policy/decoder_hyundai.rs"]
mod decoder_hyundai;
#[path = "../examples/radarcan_policy/decoder_parser.rs"]
mod decoder_parser;
#[path = "../examples/radarcan_policy/decoder_settings.rs"]
mod decoder_settings;
use openpilot_radarcan::{databases::Databases, numerics::Numerics};
use serde_json::{json, Value};
fn compare(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(
            a.as_f64().unwrap().to_bits(),
            b.as_f64().unwrap().to_bits(),
            "{path}"
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
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
#[test]
fn complete_factory_bus_modes_scc_gaps_and_holds_match_original_source() {
    let fixture: Value = serde_json::from_slice(
        &zstd::stream::decode_all(&include_bytes!("fixtures/bosch-factory-final.json.zst")[..])
            .unwrap(),
    )
    .unwrap();
    let mut numerics = Numerics::load(std::path::Path::new(
        &std::env::var("RADARCAN_NUMERICS_DIR")
            .expect("explicit verified ILP64 directory required"),
    ))
    .unwrap();
    let dbc = std::env::var("RADARCAN_DBC_DIR")
        .expect("explicit source-matched DBC asset directory required");
    let requests = fixture["requests"].as_array().unwrap();
    assert_eq!(requests.len(), 12);
    let mut transitions = 0;
    for (request, expected) in requests.iter().zip(fixture["expected"].as_array().unwrap()) {
        let mut stdout = String::new();
        transitions += request["actions"].as_array().unwrap().len();
        let result = decoder::trace(
            request.clone(),
            &mut Databases::new(dbc.clone().into()),
            &mut numerics,
            &mut stdout,
        )
        .unwrap();
        let actual = json!({"name":request["name"],"result":result,"stdout":stdout,"stderr":""});
        compare(&actual, expected, request["name"].as_str().unwrap());
    }
    assert_eq!(transitions, 3600);
}
