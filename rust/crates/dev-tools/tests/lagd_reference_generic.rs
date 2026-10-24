#![cfg(feature = "lagd-tools-native")]
//! Independent admission captures verified against the source-derived native reference.
use openpilot_dev_tools::lagd_reference::numeric;
use openpilot_log_format::JsonValue;
use serde_json::Value;

#[path = "support/lagd_expected.rs"]
mod expected;

fn budget(name: &str) -> (f64, f64) {
    match name {
        "pose_and_smoothing" => (1e-12, 1e-12),
        "correlation" => (1e-9, 1e-9),
        "lag_and_block_statistics" => (1e-8, 1e-8),
        _ => panic!("unrecognized original budget"),
    }
}

#[test]
fn original_121_generic_dtype_index_mask_text_and_failure_boundaries() {
    let corpus = expected::original("generic-original.json.gz");
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 121);
    let mut mismatches = Vec::new();
    for row in cases {
        let name = row["case"].as_str().unwrap();
        let input = JsonValue::parse(row["input"].as_str().unwrap()).unwrap();
        let actual = numeric(&input);
        if let Some(output) = row["output"].as_str() {
            let want = JsonValue::parse(output).unwrap();
            match actual {
                Ok(actual) => {
                    if let Err(error) = expected::compare(
                        &want,
                        &actual,
                        budget(row["budget"].as_str().unwrap()),
                        false,
                        name,
                    ) {
                        mismatches.push(error);
                    }
                }
                Err(error) => {
                    mismatches.push(format!("{name}: original succeeds, native failed: {error}"))
                }
            }
        } else if actual.is_ok() {
            // Exact original error class/message and warnings remain captured.
            // The native library's Result boundary must preserve rejection; no
            // Python traceback formatting contract is invented for this Rust API.
            mismatches.push(format!(
                "{name}: original {} rejected, native admitted",
                row["error"]["type"]
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} mismatches:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}

#[test]
fn independently_captured_parent_weighted_masks_integer_storage_and_string_rejection() {
    let corpus = expected::original("dtype-parent-original.json");
    let rows = corpus.as_array().unwrap();
    assert_eq!(rows.len(), 5);
    for row in rows {
        let input = JsonValue::parse(&serde_json::to_string(&row["input"]).unwrap()).unwrap();
        let actual = numeric(&input);
        if !row["error"].is_null() {
            assert!(actual.is_err(), "{}", row["name"]);
        } else {
            let value: &Value = &row["result"];
            let expected = JsonValue::parse(&serde_json::to_string(value).unwrap()).unwrap();
            let limit = if row["input"]["kind"] == "correlate" {
                1e-9
            } else if row["input"]["kind"] == "smooth" {
                1e-12
            } else {
                1e-8
            };
            expected::compare(
                &expected,
                &actual.unwrap(),
                (limit, limit),
                false,
                row["name"].as_str().unwrap(),
            )
            .unwrap();
        }
    }
}

#[test]
fn original_padding_scalar_kinds_and_peak_integer_lane_arithmetic() {
    use sha2::{Digest, Sha256};
    let raw = include_bytes!("fixtures/lagd-tools/scalar-kind-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(raw)),
        "b8deac5ca37c149a72b7a35a4d1e90bc589458a1cf09b04eceeb3355db4bbbc8"
    );
    let original: Value = serde_json::from_slice(raw).unwrap();
    let rows = original["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 6);
    let mut mismatches = Vec::new();
    for row in rows {
        let name = row["case"].as_str().unwrap();
        let input = JsonValue::parse(row["input"].as_str().unwrap()).unwrap();
        let want = JsonValue::parse(row["output"].as_str().unwrap()).unwrap();
        match numeric(&input) {
            Ok(actual) => {
                if name.starts_with("padding") {
                    // Python == admits bool/int equality. Preserve the actual
                    // early-return JSON scalar kind as well as its value.
                    if actual.to_json().unwrap() != want.to_json().unwrap() {
                        mismatches.push(format!("{name}: returned scalar kinds changed"));
                    }
                } else if let Err(error) =
                    expected::compare(&want, &actual, (1e-8, 1e-8), false, name)
                {
                    mismatches.push(error);
                }
            }
            Err(error) => {
                mismatches.push(format!("{name}: original succeeds, native failed: {error}"))
            }
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn original_numpy_integer_promotion_distinguishes_float_unsigned_and_object_arithmetic() {
    use sha2::{Digest, Sha256};
    let raw = include_bytes!("fixtures/lagd-tools/inference-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(raw)),
        "c00c0cf983cc82b4b5bd39ca1b29d40b5b1b66797c0427e14a0e9872ae9a3112"
    );
    let corpus: Value = serde_json::from_slice(raw).unwrap();
    let rows = corpus["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 6);
    for row in rows {
        let input = JsonValue::parse(row["input"].as_str().unwrap()).unwrap();
        let expected = JsonValue::parse(row["output"].as_str().unwrap()).unwrap();
        let actual = numeric(&input).unwrap_or_else(|error| panic!("{}: {error}", row["case"]));
        expected::compare(
            &expected,
            &actual,
            (1e-8, 1e-8),
            false,
            row["case"].as_str().unwrap(),
        )
        .unwrap();
    }
}
