//! Unchanged original journald tool policy captures, independent of daemon code.
use openpilot_dev_tools::journald_check::{entry_json, valid_records};
use openpilot_log_format::JsonValue;
use sha2::{Digest, Sha256};
#[test]
fn independent_original_policy_and_seeded_generator() {
    let raw = include_bytes!("fixtures/journald-check/policy-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(raw)),
        "739e372a3b7a8f04a9e6f319ebb72079a52aa8e8084c8fa5428b93b3a76cc7f5"
    );
    let corpus: serde_json::Value = serde_json::from_slice(raw).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    let generated = valid_records().unwrap();
    assert_eq!(
        generated.len(),
        corpus["generated_count"].as_u64().unwrap() as usize
    );
    for (i, text) in generated.iter().enumerate() {
        assert_eq!(text, cases[i]["input"].as_str().unwrap(), "generator {i}");
    }
    for case in cases {
        let observed = entry_json(case["input"].as_str().unwrap());
        if let Some(expected) = case["expected_json"].as_str() {
            let actual = observed.unwrap();
            let expected = JsonValue::parse(expected).unwrap().to_json().unwrap();
            assert_eq!(actual, expected, "case {}", case["name"]);
        } else {
            let error = observed.unwrap_err();
            assert_eq!(error.kind, case["error"]["type"], "case {}", case["name"]);
            assert_eq!(
                error.message, case["error"]["message"],
                "case {}",
                case["name"]
            );
        }
    }
}
