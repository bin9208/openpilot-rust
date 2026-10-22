#![cfg(target_os = "linux")]

use openpilot_dev_tools::{
    manager_qa::{self, Options},
    qa_process::{CommandSpec, NativeRunner, Runner},
};
use std::{path::PathBuf, time::Duration};

#[test]
#[ignore = "hosted original-source and fresh native manager/ZMQ gate"]
fn fresh_manager_trace_matches_independent_seven_cases_and_four_log_packets() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let binary =
        PathBuf::from(std::env::var_os("MANAGER_QA_BINARY").expect("MANAGER_QA_BINARY required"));
    let output =
        PathBuf::from(std::env::var_os("MANAGER_QA_OUTPUT").expect("MANAGER_QA_OUTPUT required"));
    let checker = PathBuf::from(
        std::env::var_os("MANAGER_QA_CHECKER_BINARY").expect("MANAGER_QA_CHECKER_BINARY required"),
    );
    for (args, expected) in [(vec!["--help"], 0), (vec![], 2), (vec!["--unknown"], 2)] {
        let mut argv = vec![checker.to_str().unwrap().to_owned()];
        argv.extend(args.into_iter().map(str::to_owned));
        let mut runner = NativeRunner;
        let status = runner
            .run(&CommandSpec::new(argv, Duration::from_secs(5)))
            .unwrap();
        assert_eq!(status.code, expected);
    }
    let result = manager_qa::run(&Options {
        root,
        binary,
        output,
        require_retired: false,
    })
    .unwrap();
    assert_eq!(result["cases"].as_object().unwrap().len(), 7);
    assert_eq!(
        result["logging"]["full_packets"].as_array().unwrap().len(),
        4
    );
    assert!(!result["source_roster_complete"].as_bool().unwrap());
}
