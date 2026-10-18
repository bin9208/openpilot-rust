#![cfg(feature = "lagd-tools-native")]
//! Exact unchanged-source constructor bytes, never candidate-generated inputs.
use openpilot_dev_tools::lagd_frames::{generate, messages, rewrite_timestamp};
use openpilot_log_format::JsonValue;
#[path = "support/lagd_expected.rs"]
mod expected;

#[test]
fn all_3703_original_frame_batches_and_ordering_timestamps_match() {
    let mut count = 0;
    for mode in ["normal", "recovery", "ordering"] {
        let corpus = expected::original(&format!("loop-{mode}-original.json.gz"));
        for (index, frame) in corpus["frames"].as_array().unwrap().iter().enumerate() {
            let mut actual = messages(index as i64, mode).unwrap();
            if mode == "ordering" {
                actual.reverse();
                for (offset, bytes) in actual.iter_mut().enumerate() {
                    *bytes = rewrite_timestamp(bytes, offset as u64 * 1000).unwrap();
                }
            }
            assert_eq!(
                serde_json::to_value(actual).unwrap(),
                frame["messages"],
                "{mode}[{index}]"
            );
            count += 1;
        }
    }
    assert_eq!(count, 3703);
}

#[test]
fn original_61_constructor_generic_scalar_and_schema_boundaries() {
    let corpus = expected::original("wire-neighbors-original.json.gz");
    let rows = corpus["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 61);
    for (index, row) in rows.iter().enumerate() {
        let input = JsonValue::parse(row["input"].as_str().unwrap()).unwrap();
        let actual = generate(&input);
        if let Some(want) = row.get("output") {
            let want = JsonValue::parse(&serde_json::to_string(want).unwrap()).unwrap();
            expected::compare(&want, &actual.unwrap(), (0., 0.), true, &index.to_string()).unwrap();
        } else {
            assert!(actual.is_err(), "original schema rejected {index}: {row}");
        }
    }
    assert!(corpus["source_pins_sha256"].is_string());
}
