#[path = "support/driving_cli.rs"]
mod cli;
use openpilot_dev_tools::driving_daemon_check::Options;
use std::{env, path::PathBuf};

fn required(name: &str) -> PathBuf {
    PathBuf::from(env::var_os(name).unwrap_or_else(|| panic!("cloud must set {name}")))
}

#[test]
fn full_six_scenario_live_ipc_matches_independent_original() {
    let options = Options {
        binary: required("DRIVING_DAEMON_NATIVE_BINARY"),
        catalog: required("DRIVING_DAEMON_CATALOG"),
        original_dir: required("DRIVING_ORIGINAL_OBSERVATION"),
        original_manifest_sha256: env::var("DRIVING_ORIGINAL_MANIFEST_SHA256")
            .expect("cloud must pin corrected original-only capture"),
        collector: None,
        output: required("DRIVING_DAEMON_OUTPUT"),
        root: required("DRIVING_DAEMON_ROOT"),
        peer: env::var_os("DRIVING_DAEMON_PEER").map(PathBuf::from),
        compiler: env::var_os("DRIVING_DAEMON_COMPILER").map(PathBuf::from),
    };
    let mut args = vec![
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
    let result = cli::run(
        std::path::Path::new(env!("CARGO_BIN_EXE_check-driving-daemon")),
        args,
        &options.output,
    );
    let runs = result["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 6);
    assert_eq!(
        runs.iter()
            .map(|run| run["messages"].as_u64().unwrap())
            .sum::<u64>(),
        114
    );
    assert_eq!(
        runs.iter()
            .map(|run| run["frames"].as_u64().unwrap())
            .sum::<u64>(),
        40
    );
    assert_eq!(
        runs.iter()
            .map(|run| run["trace"].as_array().unwrap().len())
            .sum::<usize>(),
        40
    );
    assert!(options.output.join("report.json").is_file());
}
