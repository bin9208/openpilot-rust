use openpilot_dev_tools::driver_daemon_check::{parse, run, Options, OriginalCapture};
use std::{env, ffi::OsString, fs, path::PathBuf};

fn required(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("cloud must set {name}"))
}

fn options(output: PathBuf) -> Options {
    Options {
        binary: required("DRIVER_DAEMON_BINARY").into(),
        catalog: required("DRIVER_DAEMON_CATALOG").into(),
        collector: None,
        peer: required("DRIVER_DAEMON_NO_COLLECTOR_PEER").into(),
        models: required("DRIVER_DAEMON_MODELS").into(),
        original_dir: required("DRIVER_DAEMON_NO_COLLECTOR_ORIGINAL_CAPTURE").into(),
        original_manifest_sha256: required("DRIVER_DAEMON_NO_COLLECTOR_MANIFEST_SHA256"),
        source_commit: required("DRIVER_DAEMON_SOURCE_COMMIT"),
        output,
        root: required("DRIVER_DAEMON_ROOT").into(),
    }
}

#[test]
fn original_no_collector_four_case_capture_is_independent_and_sealed() {
    let original = OriginalCapture::open(
        &required("DRIVER_DAEMON_NO_COLLECTOR_ORIGINAL_CAPTURE").into(),
        &required("DRIVER_DAEMON_NO_COLLECTOR_MANIFEST_SHA256"),
        &required("DRIVER_DAEMON_SOURCE_COMMIT"),
    )
    .unwrap();
    for (width, raw, frames) in [(1344, 1, 4), (1344, 0, 1), (1928, 1, 4), (1928, 0, 1)] {
        let prefix = format!("{width}-raw-{raw}");
        let report: serde_json::Value =
            serde_json::from_slice(&original.read(&format!("{prefix}/report.json")).unwrap())
                .unwrap();
        assert_eq!(report.get("logging"), Some(&serde_json::Value::Null));
        assert_eq!(report["frames"].as_u64(), Some(frames));
        assert_eq!(report["compared_fields"].as_u64(), Some(frames * 47));
        for index in 0..=frames {
            assert!(!original
                .read(&format!("{prefix}/reference/expected-{index}.bin"))
                .unwrap()
                .is_empty());
        }
    }
    original.verify().unwrap();
}

#[test]
fn native_cli_accepts_omitted_collector_without_losing_other_required_evidence() {
    let args = [
        "--binary",
        "/private/driver",
        "--catalog",
        "/private/catalog",
        "--models",
        "/private/models",
        "--output",
        "/private/output",
        "--native-peer",
        "/private/peer",
        "--native-original-dir",
        "/private/original",
        "--native-original-manifest-sha256",
        "a",
        "--native-source-commit",
        "b",
        "--native-root",
        "/private/root",
    ]
    .map(OsString::from);
    let selected = parse(&args).unwrap().unwrap();
    assert!(selected.collector.is_none());
    assert_eq!(selected.peer, PathBuf::from("/private/peer"));
}

#[test]
fn mismatched_original_collector_mode_is_rejected_before_output_creation() {
    let temporary = tempfile::tempdir().unwrap();
    let mut no_collector = options(temporary.path().join("no-collector"));
    no_collector.original_dir = required("DRIVER_DAEMON_HISTORICAL_COLLECTOR_CAPTURE").into();
    no_collector.original_manifest_sha256 =
        required("DRIVER_DAEMON_HISTORICAL_COLLECTOR_MANIFEST_SHA256");
    no_collector.source_commit = required("DRIVER_DAEMON_HISTORICAL_COLLECTOR_SOURCE_COMMIT");
    assert_eq!(
        run(&no_collector).unwrap_err().to_string(),
        "driver original capture collector mode differs from native option"
    );
    assert!(!no_collector.output.exists());

    let mut unexpected_collector = options(temporary.path().join("unexpected-collector"));
    unexpected_collector.collector = Some(required("DRIVER_DAEMON_COLLECTOR").into());
    assert_eq!(
        run(&unexpected_collector).unwrap_err().to_string(),
        "driver original capture collector mode differs from native option"
    );
    assert!(!unexpected_collector.output.exists());
}

#[test]
fn actual_native_driver_replays_all_four_without_collector() {
    let options = options(required("DRIVER_DAEMON_NO_COLLECTOR_NATIVE_OUTPUT").into());
    let report = run(&options).unwrap();
    assert!(report["inputs"]["collector"].is_null());
    assert!(!report["inputs"]["imported_python_sources"]
        .as_array()
        .unwrap()
        .is_empty());
    let runs = report["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 4);
    for (run, frames) in runs.iter().zip([4, 1, 4, 1]) {
        assert_eq!(run["frames"].as_u64(), Some(frames));
        assert_eq!(run["compared_fields"].as_u64(), Some(frames * 47));
        assert_eq!(run.get("logging"), Some(&serde_json::Value::Null));
        assert_eq!(run["packets"].as_array().unwrap().len(), frames);
        let camera = run["camera"][0].as_u64().unwrap();
        let raw = usize::from(run["raw_predictions"].as_bool().unwrap());
        assert!(!options
            .output
            .join(format!("{camera}-raw-{raw}/logging"))
            .exists());
    }
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(options.output.join("report.json")).unwrap()).unwrap();
    assert_eq!(report, receipt);
}
