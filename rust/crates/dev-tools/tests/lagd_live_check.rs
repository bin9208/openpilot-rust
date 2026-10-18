//! Live source checker boundaries captured before native implementation.
#![cfg(feature = "lagd-tools-native")]
use openpilot_dev_tools::lagd_live_check::{run, Kind, Options};
use serde_json::Value;
use std::{fs, path::Path};
#[path = "support/lagd_cli_original.rs"]
mod expected;
#[test]
fn original_daemon_early_peer_eof_retains_only_opened_logs() {
    let original = expected::cli();
    let row = original["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["tool"] == "daemon" && v["case"] == "valid-true")
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("output");
    let opt = Options {
        root: Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."),
        binary: "/bin/true".into(),
        peer: Some("/bin/true".into()),
        output: output.clone(),
    };
    let result = run(Kind::Daemon, &opt, &mut Vec::new());
    assert!(result.is_err());
    let mut actual = serde_json::Map::new();
    for name in ["daemon.log", "peer.log"] {
        let bytes = fs::read(output.join("learning").join(name)).unwrap();
        assert!(bytes.is_empty());
        actual.insert(format!("output/learning/{name}"), Value::Null);
    }
    assert_eq!(
        actual.keys().collect::<Vec<_>>(),
        row["files"].as_object().unwrap().keys().collect::<Vec<_>>()
    );
}
#[test]
fn original_two_params_directories_with_actual_empty_native_process() {
    let original = expected::cli();
    let row = original["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["tool"] == "params_io" && v["case"] == "valid-true")
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("result.json");
    let mut stdout = Vec::new();
    let opt = Options {
        root: Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."),
        binary: "/bin/true".into(),
        peer: None,
        output: output.clone(),
    };
    run(Kind::ParamsIo, &opt, &mut stdout).unwrap();
    assert_eq!(
        String::from_utf8(stdout).unwrap(),
        row["stdout"].as_str().unwrap()
    );
    use base64::Engine;
    let want = base64::engine::general_purpose::STANDARD
        .decode(row["files"]["output"]["data"].as_str().unwrap())
        .unwrap();
    assert_eq!(fs::read(output).unwrap(), want);
}
