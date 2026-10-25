#![cfg(feature = "lagd-tools-native")]
//! Independent original cache policy and complete schema packet projection.
use openpilot_dev_tools::lagd_packet_reference::{packet, project_event, restore};
use openpilot_log_format::JsonValue;
use serde_json::Value;
#[path = "support/lagd_expected.rs"]
mod expected;
fn node(value: &Value) -> JsonValue {
    JsonValue::parse(&serde_json::to_string(value).unwrap()).unwrap()
}
fn bytes(value: &Value) -> Option<Vec<u8>> {
    value
        .as_array()
        .map(|rows| rows.iter().map(|x| x.as_u64().unwrap() as u8).collect())
}
#[test]
fn original_14_cache_seed_remove_and_unknown_enum_contracts() {
    let corpus = expected::original("cache-original.json.gz");
    let rows = corpus.as_array().unwrap();
    assert_eq!(rows.len(), 14);
    for row in rows {
        let input = &row["input"];
        let actual = restore(
            &bytes(&input["car"]).unwrap(),
            bytes(&input["saved"]).as_deref(),
            bytes(&input["previous"]).as_deref(),
        )
        .unwrap();
        expected::compare(
            &node(&row["source"]),
            &actual,
            (0., 0.),
            false,
            row["case"].as_str().unwrap(),
        )
        .unwrap();
    }
}
#[test]
fn original_13_status_clipping_partial_ring_and_float32_packet_boundaries() {
    let corpus = expected::original("packet-original.json.gz");
    let rows = corpus.as_array().unwrap();
    assert_eq!(rows.len(), 13);
    for row in rows {
        let actual = packet(&node(&row["input"])).unwrap();
        if row["source"].get("error").is_some() {
            assert!(
                actual.is_none(),
                "schema overflow admitted: {}",
                row["case"]
            );
        } else {
            let projected = project_event(&actual.unwrap()).unwrap();
            expected::compare(
                &node(&row["source"]),
                &projected,
                (1e-7, 1e-7),
                false,
                row["case"].as_str().unwrap(),
            )
            .unwrap();
        }
    }
}
