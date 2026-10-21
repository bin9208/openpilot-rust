use openpilot_dev_tools::driving_daemon_check::{self, Options};
use std::{env, path::PathBuf};

fn required(name: &str) -> PathBuf {
    PathBuf::from(env::var_os(name).unwrap_or_else(|| panic!("cloud must set {name}")))
}

#[test]
fn six_real_driving_scenarios_retain_collector_and_source_log_parity() {
    let options = Options {
        binary: required("DRIVING_DAEMON_NATIVE_BINARY"),
        catalog: required("DRIVING_DAEMON_CATALOG"),
        original_dir: required("DRIVING_ORIGINAL_OBSERVATION"),
        original_manifest_sha256: env::var("DRIVING_ORIGINAL_MANIFEST_SHA256")
            .expect("cloud must pin the corrected original-only capture"),
        collector: Some(required("DRIVING_DAEMON_COLLECTOR")),
        output: required("DRIVING_LOGGING_OUTPUT"),
        root: required("DRIVING_DAEMON_ROOT"),
        peer: Some(required("DRIVING_DAEMON_PEER")),
        compiler: None,
    };
    let report = driving_daemon_check::run(&options).unwrap();
    let runs = report["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 6);
    assert_eq!(
        runs.iter()
            .map(|run| run["messages"].as_u64().unwrap())
            .sum::<u64>(),
        114
    );
    for run in runs {
        let logging = &run["logging"];
        assert_eq!(logging["result"], "pass");
        assert_eq!(logging["source_calls"]["result"], "pass");
        assert!(logging["runtime_timing_events"].as_u64().unwrap() > 0);
        assert_eq!(logging["collector_exit"], 0);
    }
    assert!(options.output.join("report.json").is_file());
}
