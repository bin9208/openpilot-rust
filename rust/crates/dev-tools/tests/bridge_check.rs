//! Source-derived controller boundaries. Real packets/namespaces run in CI.
use openpilot_dev_tools::bridge_check::{self as bridge, Options};
use std::{
    ffi::OsString,
    fs,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::Path,
    sync::atomic::AtomicBool,
};
fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}
#[test]
fn cli_preserves_required_and_hidden_worker_arguments() {
    assert!(bridge::parse(&args(&["--help"])).unwrap().is_none());
    for values in [
        vec![],
        vec!["--binary", "probe", "--output", "out"],
        vec!["--original", "source", "--binary", "probe"],
        vec![
            "--binary",
            "probe",
            "--output",
            "out",
            "--scenario",
            "other",
        ],
    ] {
        assert!(bridge::parse(&args(&values)).is_err());
    }
    let options = bridge::parse(&args(&[
        "--binary=probe",
        "--output=out",
        "--scenario=incoming",
    ]))
    .unwrap()
    .unwrap();
    assert_eq!(options.scenario.as_deref(), Some("incoming"));
    assert!(options.original.is_none());
}
#[test]
fn cli_retains_non_utf8_paths_and_argparse_abbreviations() {
    let raw = OsString::from_vec(b"probe-\xff".to_vec());
    let options = bridge::parse(&[
        "--ori".into(),
        "source".into(),
        "--bin".into(),
        raw.clone(),
        "--out".into(),
        "output".into(),
    ])
    .unwrap()
    .unwrap();
    assert!(options
        .binary
        .as_os_str()
        .as_bytes()
        .ends_with(raw.as_bytes()));
}
#[test]
fn packet_contract_keeps_all_numbers_and_non_text_bytes() {
    for (scenario, suffix) in [
        ("outgoing", b"\0\xffbridge".as_slice()),
        ("incoming", b"incoming\0\xff".as_slice()),
    ] {
        let packets = bridge::packets(scenario).unwrap();
        assert_eq!(packets.len(), 160);
        assert_eq!(&packets[0][..4], &[0, 0, 0, 0]);
        assert_eq!(&packets[159][..4], &[159, 0, 0, 0]);
        for (number, packet) in packets.iter().enumerate() {
            assert_eq!(
                u32::from_le_bytes(packet[..4].try_into().unwrap()) as usize,
                number
            );
            assert_eq!(&packet[4..], suffix);
        }
    }
    assert!(bridge::packets("unknown").is_err());
}
#[test]
fn no_loopback_change_in_parent_network_namespace() {
    let current = fs::read_link("/proc/self/ns/net").unwrap();
    assert!(bridge::validate_namespace(&current, &current).is_err());
}
#[test]
fn preexisting_receipt_is_not_overwritten_or_used_as_success() {
    let temp = tempfile::tempdir().unwrap();
    let receipt = temp.path().join("result.json");
    fs::write(&receipt, b"foreign-success").unwrap();
    let options = Options {
        original: Some("missing-source".into()),
        binary: "missing-native".into(),
        output: temp.path().to_owned(),
        scenario: None,
    };
    assert!(bridge::run(
        &options,
        Path::new("missing-worker"),
        &AtomicBool::new(false)
    )
    .is_err());
    assert_eq!(fs::read(receipt).unwrap(), b"foreign-success");
}
#[test]
fn cancellation_before_launch_never_creates_success() {
    let temp = tempfile::tempdir().unwrap();
    let options = Options {
        original: Some("missing-source".into()),
        binary: "missing-native".into(),
        output: temp.path().join("run"),
        scenario: None,
    };
    assert!(bridge::run(
        &options,
        Path::new("missing-worker"),
        &AtomicBool::new(true)
    )
    .is_err());
    assert!(!options.output.join("result.json").exists());
}
#[test]
fn malformed_reports_cannot_substitute_for_traffic() {
    for bad in [
        serde_json::json!({}),
        serde_json::json!({"source":{},"rust":{}}),
        serde_json::json!({"source":{"outgoing":"x","incoming":"y"},"rust":{"outgoing":"x","incoming":"y"}}),
    ] {
        assert!(bridge::compare_reports(&bad, &bad).is_err());
    }
}
#[test]
fn sealed_original_bridge_observation_rejects_single_direction_drift() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/bridge-controller/original-observation.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["historical_checker_sha256"],
        "17d5fc9f1d5dc3c59306ba0a8fdcc3bd85b216b362fbbe18fdc7e5daaa246821"
    );
    let original = &fixture["original_report"];
    bridge::compare_with_observation(original).unwrap();
    for group in ["source", "rust"] {
        for direction in ["outgoing", "incoming"] {
            let mut changed = original.clone();
            changed[group][direction] = serde_json::json!("changed traffic");
            assert!(bridge::compare_with_observation(&changed).is_err());
        }
    }
}
