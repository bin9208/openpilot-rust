#[path = "support/driving_cli.rs"]
mod cli;
use openpilot_dev_tools::driving_logging_fault_check::Options;
use std::{env, path::PathBuf};

fn required(name: &str) -> PathBuf {
    PathBuf::from(env::var_os(name).unwrap_or_else(|| panic!("cloud must set {name}")))
}

#[test]
fn unchanged_model_and_collector_recover_from_both_final_link_failures() {
    let options = Options {
        fault_binary: required("DRIVING_LOGGING_FAULT_BINARY"),
        collector: required("DRIVING_DAEMON_COLLECTOR"),
        peer: required("DRIVING_DAEMON_PEER"),
        catalog: required("DRIVING_DAEMON_CATALOG"),
        original_dir: required("DRIVING_ORIGINAL_OBSERVATION"),
        original_manifest_sha256: env::var("DRIVING_ORIGINAL_MANIFEST_SHA256")
            .expect("cloud must pin the corrected original-only capture"),
        root: required("DRIVING_DAEMON_ROOT"),
        output: required("DRIVING_LOGGING_FAULT_OUTPUT"),
    };
    let report = cli::run(
        std::path::Path::new(env!("CARGO_BIN_EXE_check-model-logging-faults")),
        vec![
            "--fault-binary".into(),
            options.fault_binary.as_os_str().to_owned(),
            "--collector".into(),
            options.collector.as_os_str().to_owned(),
            "--peer".into(),
            options.peer.as_os_str().to_owned(),
            "--catalog".into(),
            options.catalog.as_os_str().to_owned(),
            "--original-dir".into(),
            options.original_dir.as_os_str().to_owned(),
            "--original-manifest-sha256".into(),
            options.original_manifest_sha256.clone().into(),
            "--root".into(),
            options.root.as_os_str().to_owned(),
            "--output".into(),
            options.output.as_os_str().to_owned(),
        ],
        &options.output,
    );
    assert_eq!(report["result"], "pass");
    assert_eq!(report["source"]["ordinary_errno"], 88);
    assert_eq!(report["ordinary_exit"], 1);
    let runs = report["native"].as_array().unwrap();
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0]["mode"], "timing-error");
    assert_eq!(runs[1]["mode"], "timing-full");
    for run in runs {
        assert_eq!(run["next_interval_frames"], run["expected_next_frames"]);
        assert_eq!(run["model_messages"], 33);
    }
    assert!(options.output.join("report.json").is_file());
}
