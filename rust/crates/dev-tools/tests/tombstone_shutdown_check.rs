//! Actual held retrace and signal cleanup checks; original source supplies policy.
#![cfg(all(feature = "tombstone-tools-native", feature = "sdk-receiver-native"))]
use openpilot_dev_tools::tombstone_shutdown_check as shutdown;
use openpilot_log_format::JsonView;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}
fn environment() -> BTreeMap<OsString, OsString> {
    std::env::vars_os().collect()
}
fn artifact(key: &str) -> PathBuf {
    let path = PathBuf::from(
        std::env::var_os(key)
            .unwrap_or_else(|| panic!("{key} must name a current verified native artifact")),
    );
    let path = path.canonicalize().unwrap();
    assert!(fs::read(&path).unwrap().starts_with(b"\x7fELF"));
    path
}
#[test]
fn original_shutdown_argparse_observe_runner_remainder_and_raw_paths() {
    use std::os::unix::ffi::OsStringExt;
    let temp = tempfile::tempdir().unwrap();
    let raw = OsString::from_vec(b"runner-\xff".to_vec());
    let args = [
        "--observe".into(),
        "daemon".into(),
        "binding".into(),
        "output".into(),
        "--runner".into(),
        raw.clone(),
        "--observe".into(),
    ];
    let shutdown::Command::Run(options) =
        shutdown::parse(&args, temp.path(), &environment()).unwrap()
    else {
        panic!("normal args selected help")
    };
    assert!(options.observe);
    assert_eq!(options.binary, temp.path().join("daemon"));
    assert_eq!(options.params_binding, temp.path().join("binding"));
    assert_eq!(options.output, temp.path().join("output"));
    assert_eq!(options.runner, vec![raw, OsString::from("--observe")]);
    assert!(matches!(
        shutdown::parse(&["--help".into()], temp.path(), &environment()).unwrap(),
        shutdown::Command::Help
    ));
    for count in 0..3 {
        assert!(shutdown::parse(
            &[OsString::from("daemon"), "binding".into(), "out".into()][..count],
            temp.path(),
            &environment()
        )
        .is_err());
    }
}
#[test]
fn original_source_sigint_and_two_native_stops_preserve_budget_crash_and_owned_descendants() {
    let temp = tempfile::tempdir().unwrap();
    let options = shutdown::Options {
        binary: artifact("TOMBSTONE_DAEMON_BINARY"),
        params_binding: artifact("TOMBSTONE_PARAMS_BINDING"),
        output: temp.path().join("shutdown"),
        observe: false,
        runner: vec![],
        environment: environment(),
    };
    let report = shutdown::run(&root(), &options, &artifact("TOMBSTONE_LIVE_WORKER")).unwrap();
    assert!(report.get("result").unwrap().text_eq("PASS"));
    let JsonView::Array(cases) = report.get("cases").unwrap().view() else {
        panic!("case array")
    };
    assert_eq!(cases.len(), 3);
    for (case, (kind, signal, code)) in cases.iter().zip([
        ("source", "SIGINT", -2),
        ("native", "SIGINT", 0),
        ("native", "SIGTERM", 0),
    ]) {
        assert!(case.get("kind").unwrap().text_eq(kind));
        assert!(case.get("signal").unwrap().text_eq(signal));
        assert_eq!(
            case.get("exit_code").unwrap().to_json().unwrap(),
            code.to_string()
        );
        assert!(matches!(
            case.get("exited_within_5_seconds").unwrap().view(),
            JsonView::Bool(true)
        ));
        let seconds = case
            .get("seconds")
            .unwrap()
            .to_json()
            .unwrap()
            .parse::<f64>()
            .unwrap();
        assert!(seconds < 5.0, "stop timeout changed: {seconds}");
        assert!(matches!(
            case.get("crash_retained").unwrap().view(),
            JsonView::Bool(true)
        ));
        assert!(matches!(
            case.get("crash_copied").unwrap().view(),
            JsonView::Bool(false)
        ));
        assert_eq!(case.get("http_events").unwrap().to_json().unwrap(), "0");
        let JsonView::Array(before) = case.get("before").unwrap().view() else {
            panic!("acknowledged process array")
        };
        assert!(before.len() >= 3);
        if kind == "native" {
            assert!(
                matches!(case.get("active_descendants_after_stop").unwrap().view(),JsonView::Array(values)if values.is_empty())
            );
        }
    }
    assert_eq!(
        fs::read_to_string(options.output.join("manifest.json")).unwrap(),
        shutdown::format_report(&report).unwrap() + "\n"
    );
}

#[test]
fn source_preflight_failure_invalidates_prior_success_without_starting_worker() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("evidence");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("manifest.json"), b"{\"result\":\"PASS\"}\n").unwrap();
    let options = shutdown::Options {
        binary: "must-not-run".into(),
        params_binding: "missing".into(),
        output: output.clone(),
        observe: false,
        runner: vec![],
        environment: environment(),
    };
    assert!(shutdown::run(root.path(), &options, Path::new("missing-worker")).is_err());
    assert!(!output.join("manifest.json").exists());
    assert_eq!(fs::read_dir(output).unwrap().count(), 0);
}
