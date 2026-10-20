//! Sealed original-only config corpus and fresh same-ELF present/absent proof.
#![cfg(feature = "bluetooth-config-native")]
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::PathBuf};

fn required(name: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("required Bluetooth config proof missing: {name}"))
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn fixture(name: &str) -> Vec<u8> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/bluetooth-config/original-observation-39507901490");
    fs::read(root.join(name)).unwrap()
}

#[test]
fn sealed_original_and_two_fresh_native_lanes_keep_full_253_case_corpus() {
    let original = [
        (
            "input.jsonl",
            "26af3364d60611668ea429ec9d3e0eb73506f0fcd3b6c444e6c156fcb0d1638e",
        ),
        (
            "original.jsonl",
            "0f0c272b477322b200664232afa4d1886c7d25b142c78ba2b5be6b3cac728b97",
        ),
        (
            "result.json",
            "3fba79c8319615ab106b92b9064e90097d2956184bec00ffa41f2a853606d74f",
        ),
    ];
    let provenance_bytes = fixture("provenance.json");
    assert_eq!(
        digest(&provenance_bytes),
        "ca7bb11da596c77e4f0dac309e240a6fa3bf5a8dfc97b04969e6aedb2ae51bd7"
    );
    let provenance: Value = serde_json::from_slice(&provenance_bytes).unwrap();
    assert_eq!(provenance["candidate_executed_for_expectations"], false);
    assert_eq!(
        provenance["checked_out_source"],
        "9845db2db390bc904401a3794b723b510e3bef84"
    );
    assert_eq!(
        provenance["workflow_head_sha"],
        "6b1f6bb015cc9497237beb39fb56c5e7a37b274c"
    );
    assert_eq!(
        provenance["source_sha256"],
        "a8f85f1076773e1e5938e66064e3bb1cd0ea060d7d60df528365108fbcd2d679"
    );
    let present = required("BLUETOOTH_CONFIG_PRESENT_OUTPUT");
    let absent = required("BLUETOOTH_CONFIG_ABSENT_OUTPUT");
    let absent_root = required("BLUETOOTH_CONFIG_ABSENT_ROOT");
    assert!(matches!(
        fs::symlink_metadata(absent_root.join("rust/tools/check_bluetooth_config.py")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    ));
    for (name, hash) in original {
        let bytes = fixture(name);
        assert_eq!(digest(&bytes), hash, "original {name}");
        assert_eq!(provenance["original_sha256"][name], hash);
        if name != "result.json" {
            assert_eq!(
                fs::read(present.join(name)).unwrap(),
                bytes,
                "present {name}"
            );
            assert_eq!(fs::read(absent.join(name)).unwrap(), bytes, "absent {name}");
        }
    }
    let original: Value = serde_json::from_slice(&fixture("result.json")).unwrap();
    let present_report: Value =
        serde_json::from_slice(&fs::read(present.join("result.json")).unwrap()).unwrap();
    let absent_report: Value =
        serde_json::from_slice(&fs::read(absent.join("result.json")).unwrap()).unwrap();
    assert_eq!(original["cases"], 253);
    assert_eq!(original["accepted"], 97);
    for key in ["cases", "accepted", "source_sha256", "binary_sha256"] {
        if key == "binary_sha256" {
            assert_eq!(present_report[key], absent_report[key]);
        } else {
            assert_eq!(original[key], present_report[key], "present {key}");
            assert_eq!(original[key], absent_report[key], "absent {key}");
        }
    }
    assert_eq!(present_report, absent_report);
    assert_eq!(
        present_report["binary_sha256"],
        digest(&fs::read(required("BLUETOOTH_CONFIG_FIXTURE")).unwrap())
    );
    assert_eq!(
        present_report["input_sha256"],
        digest(&fixture("input.jsonl"))
    );
    assert_eq!(
        present_report["original_sha256"],
        digest(&fixture("original.jsonl"))
    );
    for name in ["native.jsonl", "native.stderr"] {
        assert_eq!(
            fs::read(present.join(name)).unwrap(),
            fs::read(absent.join(name)).unwrap(),
            "{name}"
        );
    }
    for (present_name, absent_name) in [
        (
            "BLUETOOTH_CONFIG_PRESENT_STDOUT",
            "BLUETOOTH_CONFIG_ABSENT_STDOUT",
        ),
        (
            "BLUETOOTH_CONFIG_PRESENT_STDERR",
            "BLUETOOTH_CONFIG_ABSENT_STDERR",
        ),
    ] {
        assert_eq!(
            fs::read(required(present_name)).unwrap(),
            fs::read(required(absent_name)).unwrap()
        );
    }
}

fn audit_exec(trace: &str, output: &str, checker: &str, candidate: &str) {
    let expected = BTreeMap::from([
        (
            (
                checker.to_owned(),
                vec![
                    checker.to_owned(),
                    "--binary".into(),
                    candidate.to_owned(),
                    "--output".into(),
                    output.to_owned(),
                ],
            ),
            1,
        ),
        ((candidate.to_owned(), vec![candidate.to_owned()]), 1),
    ]);
    let opening = regex::Regex::new(r"\bexecve(?:at)?\(").unwrap();
    let invocation =
        regex::Regex::new(r#"(?m)\bexecve\(("(?:[^"\\]|\\.)*"), (\[.*\]), .*\) = 0$"#).unwrap();
    assert_eq!(
        opening.find_iter(&trace).count(),
        2,
        "unexpected exec attempt"
    );
    let mut actual = BTreeMap::new();
    for capture in invocation.captures_iter(&trace) {
        let program: String = serde_json::from_str(&capture[1]).unwrap();
        let argv: Vec<String> = serde_json::from_str(&capture[2]).unwrap();
        *actual.entry((program, argv)).or_insert(0) += 1;
    }
    assert_eq!(actual, expected, "failed, opaque or unexpected exec/argv");
}

#[test]
fn both_lanes_execute_exactly_the_same_sealed_checker_and_fixture() {
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let checker = fs::canonicalize(required("BLUETOOTH_CONFIG_CHECKER")).unwrap();
    let candidate = fs::canonicalize(required("BLUETOOTH_CONFIG_FIXTURE")).unwrap();
    let model =
        fs::canonicalize(repository.join("openpilot/selfdrive/carrot/bluetooth/model.py")).unwrap();
    let seals = fs::read_to_string(required("BLUETOOTH_CONFIG_ARTIFACTS_SHA256")).unwrap();
    let mut sealed = BTreeMap::new();
    for line in seals.lines() {
        let (hash, path) = line.split_once("  ").expect("sha256sum artifact line");
        let path = PathBuf::from(path);
        let path = fs::canonicalize(if path.is_absolute() {
            path
        } else {
            repository.join(path)
        })
        .unwrap();
        assert!(
            sealed.insert(path, hash).is_none(),
            "duplicate artifact seal"
        );
    }
    assert_eq!(sealed.len(), 3, "checker, fixture and model seals");
    for path in [&checker, &candidate, &model] {
        assert_eq!(
            sealed.get(path).copied().expect("expected artifact seal"),
            digest(&fs::read(path).unwrap())
        );
    }
    let checker = checker.to_str().unwrap();
    let candidate = candidate.to_str().unwrap();
    for (trace_name, output_name) in [
        (
            "BLUETOOTH_CONFIG_PRESENT_EXEC",
            "BLUETOOTH_CONFIG_PRESENT_OUTPUT",
        ),
        (
            "BLUETOOTH_CONFIG_ABSENT_EXEC",
            "BLUETOOTH_CONFIG_ABSENT_OUTPUT",
        ),
    ] {
        let trace = fs::read_to_string(required(trace_name)).unwrap();
        let output = required(output_name);
        audit_exec(&trace, output.to_str().unwrap(), checker, candidate);
    }
}
