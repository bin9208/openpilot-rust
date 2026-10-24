#![cfg(all(target_os = "linux", feature = "hardwared-qa-native"))]

use openpilot_dev_tools::hardwared_qa::{self, DaemonOptions, TraceOptions};
use std::path::PathBuf;

fn required(name: &str) -> PathBuf {
    std::env::var_os(name)
        .unwrap_or_else(|| panic!("{name} required"))
        .into()
}

#[test]
#[ignore = "hosted original hardwared trace capture and current-host native trace ELF required"]
fn same_trace_elf_matches_all_original_hardwared_cases() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let report = hardwared_qa::run_trace(&TraceOptions {
        source_root: root.join("../../..").canonicalize().unwrap(),
        binary: required("HARDWARED_TRACE_BINARY"),
        output: required("HARDWARED_TRACE_OUTPUT"),
        runner: std::env::var("HARDWARED_TRACE_RUNNER_JSON")
            .ok()
            .map(|value| serde_json::from_str::<Vec<String>>(&value).expect("runner JSON"))
            .map(|values| values.into_iter().map(Into::into).collect())
            .unwrap_or_default(),
        require_retired: std::env::var_os("HARDWARED_REQUIRE_RETIRED")
            .is_some_and(|value| value == "1"),
    })
    .unwrap();
    assert_eq!(report["result"], "PASS");
    assert_eq!(report["case_count"], 12);
    assert_eq!(report["observation_count"], 890);
}

#[test]
#[ignore = "hosted original hardwared checker contract and current-host daemon ELF required"]
fn actual_hardwared_preserves_live_ipc_lifecycle_and_shutdown_contract() {
    let report = hardwared_qa::run_daemon(&DaemonOptions {
        source_root: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap(),
        binary: required("HARDWARED_DAEMON_BINARY"),
        root: required("HARDWARED_DAEMON_ROOT"),
        output: required("HARDWARED_DAEMON_OUTPUT"),
        launcher: required("HARDWARED_PROCESS_LAUNCHER"),
        require_retired: std::env::var_os("HARDWARED_REQUIRE_RETIRED")
            .is_some_and(|value| value == "1"),
    })
    .unwrap();
    assert_eq!(report["result"], "PASS");
    assert_eq!(report["exit_code"], 0);
    assert_eq!(report["peer"]["transitions"].as_array().unwrap().len(), 5);
}
