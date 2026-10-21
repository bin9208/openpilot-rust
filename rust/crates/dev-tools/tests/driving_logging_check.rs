#[path = "support/driving_cli.rs"]
mod cli;
use openpilot_dev_tools::driving_daemon_check::Options;
use std::{env, path::PathBuf};

fn required(name: &str) -> PathBuf {
    PathBuf::from(env::var_os(name).unwrap_or_else(|| panic!("cloud must set {name}")))
}

#[test]
fn six_real_driving_scenarios_retain_collector_and_source_log_parity() {
    let options = Options {
        source_policy:
            openpilot_dev_tools::driving_daemon_check::source_policy::SourcePolicy::parse(
                &env::var("DRIVING_SOURCE_POLICY").unwrap_or_else(|_| "present".to_owned()),
            )
            .expect("cloud source policy must be explicit present or retired-driving"),
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
    let mut args = vec![
        "--source-policy".into(),
        options.source_policy.as_str().into(),
        "--binary".into(),
        options.binary.as_os_str().to_owned(),
        "--catalog".into(),
        options.catalog.as_os_str().to_owned(),
        "--original-dir".into(),
        options.original_dir.as_os_str().to_owned(),
        "--original-manifest-sha256".into(),
        options.original_manifest_sha256.clone().into(),
        "--output".into(),
        options.output.as_os_str().to_owned(),
        "--root".into(),
        options.root.as_os_str().to_owned(),
    ];
    if let Some(collector) = &options.collector {
        args.extend(["--collector".into(), collector.as_os_str().to_owned()]);
    }
    if let Some(peer) = &options.peer {
        args.extend(["--peer".into(), peer.as_os_str().to_owned()]);
    }
    if let Some(compiler) = &options.compiler {
        args.extend(["--compiler".into(), compiler.as_os_str().to_owned()]);
    }
    let report = cli::run(
        std::path::Path::new(env!("CARGO_BIN_EXE_check-driving-daemon")),
        args,
        &options.output,
    );
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
