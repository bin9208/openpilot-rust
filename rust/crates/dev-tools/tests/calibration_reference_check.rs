use openpilot_dev_tools::{
    calibration_reference_check::{check, Options},
    qa_process::NativeRunner,
};
use std::{env, path::PathBuf};

#[test]
fn actual_native_stream_matches_all_original_calibration_steps() {
    let options = Options {
        root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."),
        binary: PathBuf::from(
            env::var_os("CALIBRATION_REFERENCE_BINARY").expect("mandatory calibration_trace ELF"),
        ),
        output: PathBuf::from(
            env::var_os("CALIBRATION_REFERENCE_EVIDENCE")
                .expect("mandatory fresh evidence directory"),
        ),
    };
    let report = check(&options, &mut NativeRunner).unwrap();
    assert_eq!(report["result"], "pass");
    assert_eq!(report["steps"], 15_128);
    assert_eq!(report["expected_source_errors"], 30);
    assert_eq!(report["persistence_steps"], 14);
    assert_eq!(report["statuses"]["calibrated"], 11_744);
    assert_eq!(report["scenarios"]["long-history"], 12_402);
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(options.output.join("report.json")).unwrap())
            .unwrap();
    assert_eq!(saved, report);
    let trace = std::fs::read_to_string(options.output.join("trace.jsonl")).unwrap();
    let rows = trace.lines().collect::<Vec<_>>();
    assert_eq!(rows.len(), 15_128);
    let first: serde_json::Value = serde_json::from_str(rows[0]).unwrap();
    let last: serde_json::Value = serde_json::from_str(rows[15_127]).unwrap();
    assert_eq!(first["step"], 1);
    assert_eq!(last["step"], 15_128);
    assert!(first["scenario"].is_string() && first["state"].is_object());
    assert!(last["scenario"].is_string() && last["state"].is_object());
}
