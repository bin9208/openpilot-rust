#![cfg(feature = "lagd-tools-native")]
//! Whole original request stream and case order, not a replacement request blob.
use openpilot_dev_tools::lagd_numeric_cases::{cases, request};
use sha2::{Digest, Sha256};
use std::io::Read;
#[path = "support/lagd_expected.rs"]
mod expected;

#[test]
fn all_original_95_inputs_names_budgets_and_float_bits_are_generated() {
    // Validate immutable provenance through the common loader, then parse the
    // same captured bytes losslessly. serde_json without float_roundtrip may
    // move a captured decimal by one ULP before an exact comparison.
    let _ = expected::original("numeric-original.json.gz");
    let mut raw = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("fixtures/lagd-tools/numeric-original.json.gz").as_slice(),
    )
    .read_to_string(&mut raw)
    .unwrap();
    let original = openpilot_log_format::JsonValue::parse(&raw).unwrap();
    let openpilot_log_format::JsonView::Array(original) = original.view() else {
        panic!("original numeric cases must be an array")
    };
    let generated = cases().unwrap();
    assert_eq!(generated.len(), 95);
    assert_eq!(generated.len(), original.len());
    for (index, (actual, want)) in generated.iter().zip(original).enumerate() {
        assert_eq!(actual.name, want.get("case").unwrap().to_utf8().unwrap());
        assert_eq!(
            actual.budget,
            want.get("budget").unwrap().to_utf8().unwrap()
        );
        let want = want.get("input").unwrap();
        expected::compare(
            &want,
            &actual.input,
            (0., 0.),
            false,
            &format!("input {index}"),
        )
        .unwrap();
    }
}
#[test]
fn original_full_input_stream_is_byte_identical_with_python_number_and_object_order() {
    let compressed = include_bytes!("fixtures/lagd-tools/numeric-inputs.jsonl.gz");
    assert_eq!(
        format!("{:x}", Sha256::digest(compressed)),
        "cc6e82bacc005e3bcffe74fb1335864c8845410bd61b945c8f54ca3b31de09d7"
    );
    let mut original = Vec::new();
    flate2::read::GzDecoder::new(compressed.as_slice())
        .read_to_end(&mut original)
        .unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&original)),
        "b6c67cd28093e82ec7badb29d54b9dba385b60fa028d9ffc82da3cfe5573dce4"
    );
    assert_eq!(request().unwrap(), original);
}
