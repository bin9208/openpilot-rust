use std::{fs, process::Command};

#[test]
fn malformed_json_and_short_vectors_exit_nonzero() {
    let directory = tempfile::tempdir().unwrap();
    let request_path = directory.path().join("request.jsonl");
    let state_path = directory.path().join("state.jsonl");
    let packet_path = directory.path().join("packet.bin");
    let valid = serde_json::json!({"reset": true, "rhd_saved": false, "always_on": false,
        "too_distracted": false, "valid": true, "input": openpilot_monitoring::Input::default()});
    let mut malformed = valid.clone();
    malformed["input"]["driver"]["left"]["face_orientation"] = serde_json::json!([0.]);
    for source in [
        "{".to_owned(),
        malformed.to_string(),
        valid.to_string().replace("20.0", "NaN"),
    ] {
        fs::write(&request_path, source).unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_monitoring-probe"))
            .args([&request_path, &state_path, &packet_path])
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(!result.stderr.is_empty());
        assert_eq!(fs::metadata(&packet_path).unwrap().len(), 0);
    }
}

#[test]
fn absent_arguments_report_usage() {
    let result = Command::new(env!("CARGO_BIN_EXE_monitoring-probe"))
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("usage: monitoring-probe"));
}
