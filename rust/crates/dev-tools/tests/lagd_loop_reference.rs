#![cfg(feature = "lagd-tools-native")]
//! Full source state snapshots, including non-publication frames.
use openpilot_dev_tools::{lagd_loop_reference::run, lagd_packet_reference::project_event};
use openpilot_log_format::JsonValue;
use serde_json::Value;
#[path = "support/lagd_expected.rs"]
mod expected;
fn node(value: &Value) -> JsonValue {
    JsonValue::parse(&serde_json::to_string(value).unwrap()).unwrap()
}
fn decoded(value: JsonValue) -> Value {
    let mut value: Value = serde_json::from_str(&value.to_json().unwrap()).unwrap();
    for row in value.as_array_mut().unwrap() {
        if !row["packet"].is_null() {
            let bytes = row["packet"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_u64().unwrap() as u8)
                .collect::<Vec<_>>();
            row["packet"] =
                serde_json::from_str(&project_event(&bytes).unwrap().to_json().unwrap()).unwrap();
        }
    }
    value
}
#[test]
fn every_original_3703_loop_state_order_mask_recovery_and_persistence_field() {
    let mut total = 0;
    for mode in ["normal", "recovery", "ordering"] {
        let corpus = expected::original(&format!("loop-{mode}-original.json.gz"));
        let actual =
            decoded(run(&node(&corpus["frames"]), &node(&corpus["configuration"])).unwrap());
        expected::compare(
            &node(&corpus["source"]),
            &node(&actual),
            (1e-7, 1e-7),
            false,
            mode,
        )
        .unwrap();
        total += actual.as_array().unwrap().len();
    }
    assert_eq!(total, 3703);
}
#[test]
fn original_16_loop_configuration_wire_unknown_topic_and_unused_field_boundaries() {
    let corpus = expected::original("loop-neighbors-original.json.gz");
    let rows = corpus["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 16);
    for row in rows {
        let actual = run(&node(&row["frames"]), &node(&row["configuration"]));
        if let Some(want) = row.get("output") {
            expected::compare(
                &node(want),
                &node(&decoded(actual.unwrap())),
                (1e-7, 1e-7),
                false,
                row["case"].as_str().unwrap(),
            )
            .unwrap();
        } else {
            assert!(actual.is_err(), "original rejects {}", row["case"]);
        }
    }
}

#[test]
fn original_25_bytes_conversion_and_empty_iterable_boundaries() {
    let corpus = expected::original("loop-admission-original.json.gz");
    let rows = corpus["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 25);
    for row in rows {
        let actual = run(&node(&row["frames"]), &node(&row["configuration"]));
        if let Some(want) = row.get("output") {
            expected::compare(
                &node(want),
                &node(&decoded(actual.unwrap())),
                (1e-7, 1e-7),
                false,
                row["case"].as_str().unwrap(),
            )
            .unwrap();
        } else {
            assert!(actual.is_err(), "original rejects {}", row["case"]);
        }
    }
}
