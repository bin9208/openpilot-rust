//! Actual unchanged-original checker contracts; candidate outputs are observations.
#![cfg(feature = "registration-tools-native")]
#[path = "support/registration_original.rs"]
mod original;
#[path = "support/registration_source_pins.rs"]
mod source_pins;
use openpilot_dev_tools::registration_check::{self as native, Command, Options, Suite};
use original::{array, assert_json, field, read, root, text, worker};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    os::unix::{ffi::OsStringExt, fs::symlink},
    path::{Path, PathBuf},
};

fn manifest() -> Value {
    let raw = fs::read(original::fixture("manifest.json")).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        "246dd73d62460dfb32618be8f46500ff961a9710964125e36933b806d3c3416f"
    );
    serde_json::from_slice(&raw).unwrap()
}
fn options(binary: PathBuf, output: PathBuf) -> Options {
    Options {
        root: root(),
        binary,
        output,
        runner: vec![],
        case: None,
        suite: Suite::All,
        environment: std::env::vars_os().collect(),
    }
}
fn probe() -> PathBuf {
    let path=PathBuf::from(std::env::var_os("REGISTRATION_BINARY").expect("prerequisite: REGISTRATION_BINARY must name the fresh source-matched registration_trace ELF"));
    assert!(
        path.is_absolute() && path.is_file(),
        "missing registration_trace prerequisite"
    );
    path
}
#[test]
fn original_capture_has_full_source_identity_and_no_candidate_or_private_key_expectations() {
    let manifest = manifest();
    assert_eq!(manifest["source_sha256"].as_object().unwrap().len(), 46);
    assert_eq!(
        manifest["supplemental_source_sha256"]
            .as_object()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(manifest["all_source_sha256"].as_object().unwrap().len(), 47);
    assert_eq!(
        manifest["counts"],
        serde_json::json!({"base":119,"hardware":12,"timeouts":3,"generic":4,"original_cli":16})
    );
    assert_eq!(manifest["candidate_executed_for_expectations"], false);
    assert_eq!(
        manifest["independent_boundary_counts"],
        serde_json::json!({"response_text":150,"response_headers":3,"unsigned_algorithm_query":1,"response_encoders":147,"cp949_scalars":17176,"utf7_encoders":265})
    );
    assert_eq!(manifest["retired_sources"], serde_json::json!([]));
    assert_eq!(
        manifest["baseline_provider_fixture"],
        "provider-baseline.json"
    );
    assert_eq!(
        manifest["runtime_expectation_fixture"],
        "source-baseline-original.json"
    );
    assert_eq!(
        manifest["extended_provider_fixture"],
        "source-original.json"
    );
    let baseline: Value =
        serde_json::from_slice(&fs::read(original::fixture("provider-baseline.json")).unwrap())
            .unwrap();
    assert_eq!(baseline["baseline_provider"]["requests"], "2.34.2");
    assert_eq!(baseline["baseline_provider"]["urllib3"], "2.7.0");
    assert_eq!(baseline["baseline_provider"]["zstandard_dist"], "0.25.0");
    assert_eq!(
        baseline["baseline_provider"]["backports_zstd_loaded"],
        false
    );
    assert_eq!(baseline["extended_provider"]["backports_zstd_loaded"], true);
    assert_eq!(
        baseline["original_checker_paired_control"]["baseline_exit"],
        0
    );
    assert_eq!(
        baseline["original_checker_paired_control"]["extended_exit"],
        1
    );
    for (name, header) in [
        ("source-baseline-original.json", "gzip, deflate, br"),
        ("source-original.json", "gzip, deflate, br, zstd"),
    ] {
        let capture = read(&original::fixture(name));
        assert_eq!(array(&field(&capture, "cases")).len(), 134);
        assert_eq!(array(&field(&capture, "generic")).len(), 4);
        for group in ["cases", "generic"] {
            for row in array(&field(&capture, group)) {
                for request in array(&field(&row, "requests")) {
                    assert_eq!(text(&field(&request, "accept_encoding")), header);
                    if let Some(algorithm) = field(&request, "query").get("algorithm") {
                        assert!(
                            algorithm.to_utf8().is_some(),
                            "the unchanged original normalizer emits a scalar verified algorithm"
                        );
                    }
                }
            }
        }
    }
    for (name, expected) in manifest["files"].as_object().unwrap() {
        let bytes = fs::read(original::fixture(name)).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            expected.as_str().unwrap(),
            "{name}"
        );
        assert!(!bytes
            .windows(b"BEGIN RSA PRIVATE KEY".len())
            .any(|v| v == b"BEGIN RSA PRIVATE KEY"));
        assert!(!bytes
            .windows(b"BEGIN EC PRIVATE KEY".len())
            .any(|v| v == b"BEGIN EC PRIVATE KEY"));
    }
}
#[test]
fn retained_source_pins_are_required_and_present_retired_helpers_remain_verified() {
    let manifest = manifest();
    let pins = manifest["all_source_sha256"].as_object().unwrap();
    let actual = native::validate_source(&root()).unwrap();
    let mut expected = manifest["all_source_sha256"].as_object().unwrap().clone();
    expected.retain(|name, _| source_pins::read(&root(), name).is_some());
    assert_json(
        &actual,
        &openpilot_log_format::JsonValue::parse(&serde_json::Value::Object(expected).to_string())
            .unwrap(),
        "all live original inputs",
    );
    let copied = tempfile::tempdir().unwrap();
    for (name, _) in pins {
        let target = copied.path().join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        if let Some(raw) = source_pins::read(&root(), name) {
            fs::write(&target, raw).unwrap();
        }
    }
    for (name, _) in pins {
        let target = copied.path().join(name);
        let Some(raw) = source_pins::read(copied.path(), name) else {
            continue;
        };
        fs::write(&target, b"mutated original source\n").unwrap();
        assert!(
            native::validate_source(copied.path()).is_err(),
            "changed {name}"
        );
        fs::remove_file(&target).unwrap();
        assert_eq!(
            native::validate_source(copied.path()).is_ok(),
            source_pins::retired(name),
            "missing {name}"
        );
        symlink("missing-source-target", &target).unwrap();
        assert!(
            native::validate_source(copied.path()).is_err(),
            "dangling {name}"
        );
        fs::remove_file(&target).unwrap();
        fs::write(target, raw).unwrap();
    }
    native::validate_source(copied.path()).unwrap();
}
#[test]
fn cli_retains_raw_paths_runner_order_abbreviations_and_timeout_over_hardware_selection() {
    let args = vec![
        OsString::from_vec(b"probe-\xff".to_vec()),
        "output".into(),
        "--r".into(),
        OsString::from_vec(b"runner-\xfe".to_vec()),
        "--runner=second".into(),
        "--ca".into(),
        "selected".into(),
        "--hard".into(),
        "--time".into(),
    ];
    let environment =
        BTreeMap::from([("SENTINEL".into(), OsString::from_vec(b"raw-\xfd".to_vec()))]);
    let Command::Run(actual) =
        native::parse(&args, Path::new("/cwd"), environment.clone()).unwrap()
    else {
        panic!("unexpected help")
    };
    assert_eq!(
        actual.binary,
        PathBuf::from(OsString::from_vec(b"/cwd/probe-\xff".to_vec()))
    );
    assert_eq!(actual.output, Path::new("/cwd/output"));
    assert_eq!(actual.runner, vec![args[3].clone(), "second".into()]);
    assert_eq!(actual.case.as_deref(), Some("selected"));
    assert!(matches!(actual.suite, Suite::Timeouts));
    assert_eq!(actual.environment, environment);
    assert!(matches!(
        native::parse(&["-hfoo".into()], Path::new("/cwd"), BTreeMap::new()).unwrap(),
        Command::Help
    ));
    assert!(matches!(
        native::parse(
            &["--help".into(), "--unknown".into()],
            Path::new("/cwd"),
            BTreeMap::new()
        )
        .unwrap(),
        Command::Help
    ));
    for args in [
        vec![],
        vec!["binary"],
        vec!["--help=x"],
        vec!["binary", "output", "--runner"],
        vec!["binary", "output", "--case"],
        vec!["binary", "output", "--unknown"],
    ] {
        assert!(
            native::parse(
                &args.iter().map(OsString::from).collect::<Vec<_>>(),
                Path::new("/cwd"),
                BTreeMap::new()
            )
            .is_err(),
            "{args:?}"
        );
    }
}
#[test]
fn actual_rsa_and_ec_pairs_compare_real_probe_against_independent_source_and_write_complete_receipts(
) {
    let temporary = tempfile::tempdir().unwrap();
    let worker = worker();
    let binary = probe();
    for name in ["rsa_signed", "ec_pkcs8_signed"] {
        let output = temporary.path().join(name);
        let mut options = options(binary.clone(), output.clone());
        options.case = Some(name.into());
        let result = native::run(&options, &worker).unwrap();
        let rows = array(&field(&result, "cases"));
        assert_eq!(rows.len(), 1);
        assert_eq!(text(&field(&rows[0], "case")), name);
        assert_eq!(text(&field(&rows[0], "result")), "PASS");
        assert_json(
            &read(&output.join("result.json")),
            &result,
            "complete success receipt",
        );
        assert_eq!(
            text(&field(&result, "binary_sha256")),
            format!("{:x}", Sha256::digest(fs::read(&binary).unwrap()))
        );
        for side in ["source", "native"] {
            assert!(
                output
                    .join(name)
                    .join(side)
                    .join("logMessage.bin")
                    .metadata()
                    .unwrap()
                    .len()
                    > 0
            );
            assert!(output
                .join(name)
                .join(side)
                .join("disk-records.json")
                .is_file());
        }
    }
}
#[test]
fn missing_or_nonzero_candidate_is_reached_only_after_source_completion_and_never_leaves_pass() {
    let temporary = tempfile::tempdir().unwrap();
    let worker = worker();
    let old = read(&original::fixture("cli-original.json"));
    for (name, binary) in [
        (
            "missing_native_after_source",
            temporary.path().join("missing"),
        ),
        ("nonzero_native_after_source", PathBuf::from("/bin/false")),
    ] {
        let expected = array(&field(&old, "cases"))
            .into_iter()
            .find(|v| text(&field(v, "name")) == name)
            .unwrap();
        let output = temporary.path().join(name);
        let mut options = options(binary, output.clone());
        options.case = Some("no_keys_overrides_saved".into());
        assert!(native::run(&options, &worker).is_err(), "{name}");
        let presence = field(&expected, "artifact_presence");
        let JsonView::Object(fields) = presence.view() else {
            panic!("missing original artifact phase")
        };
        for (path, exists) in fields {
            let path = path
                .iter()
                .copied()
                .map(char::from_u32)
                .collect::<Option<String>>()
                .unwrap();
            assert_eq!(
                output.join(&path).exists(),
                exists.to_json().unwrap() == "true",
                "{name} {path}"
            );
        }
        let source = read(&output.join("no_keys_overrides_saved/source/result.json"));
        assert!(source.get("outcome").is_some());
    }
}
#[test]
fn empty_selection_and_occupied_output_fail_without_running_candidate_or_publishing_pass() {
    let temporary = tempfile::tempdir().unwrap();
    let worker = worker();
    for (name, suite) in [
        ("does-not-exist", Suite::All),
        ("rsa_signed", Suite::Hardware),
        ("rsa_signed", Suite::Timeouts),
    ] {
        let output = temporary.path().join(format!(
            "selection-{}",
            match suite {
                Suite::All => 0,
                Suite::Hardware => 1,
                Suite::Timeouts => 2,
            }
        ));
        let mut options = options(PathBuf::from("/bin/true"), output.clone());
        options.case = Some(name.into());
        options.suite = suite;
        assert!(native::run(&options, &worker).is_err());
        assert!(output.is_dir());
        assert!(!output.join("result.json").exists());
        assert_eq!(fs::read_dir(output).unwrap().count(), 0);
    }
    let output = temporary.path().join("occupied");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("keep"), b"owned by caller").unwrap();
    assert!(native::run(
        &options(PathBuf::from("/bin/true"), output.clone()),
        &worker
    )
    .is_err());
    assert_eq!(fs::read(output.join("keep")).unwrap(), b"owned by caller");
    assert!(!output.join("result.json").exists());
}
#[test]
fn failed_admission_cannot_remove_a_preexisting_foreign_success_receipt() {
    let temporary = tempfile::tempdir().unwrap();
    let output = temporary.path().join("stale");
    fs::create_dir(&output).unwrap();
    fs::write(
        output.join("result.json"),
        b"{\"result\":\"PASS\",\"cases\":[{\"result\":\"PASS\"}]}",
    )
    .unwrap();
    fs::write(output.join("keep"), b"unrelated").unwrap();
    let mut options = options(PathBuf::from("/bin/true"), output.clone());
    options.root = temporary.path().join("invalid-source");
    assert!(native::run(&options, &worker()).is_err());
    assert_eq!(
        fs::read(output.join("result.json")).unwrap(),
        b"{\"result\":\"PASS\",\"cases\":[{\"result\":\"PASS\"}]}"
    );
    assert_eq!(fs::read(output.join("keep")).unwrap(), b"unrelated");
}
use openpilot_log_format::JsonView;

#[test]
fn successful_results_cannot_be_bound_to_replaced_probe_worker_or_changed_source() {
    use std::{
        os::unix::fs::PermissionsExt,
        time::{Duration, Instant},
    };
    let original_binary = probe();
    let original_worker = worker();
    let pins = manifest()["all_source_sha256"].as_object().unwrap().clone();
    for mutation in ["probe", "worker", "source"] {
        let temporary = tempfile::tempdir().unwrap();
        let copied = temporary.path().join("root");
        for name in pins.keys() {
            let path = copied.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            if let Some(raw) = source_pins::read(&root(), name) {
                fs::write(path, raw).unwrap();
            }
        }
        let binary = temporary.path().join("probe");
        let worker = temporary.path().join("worker");
        for (from, to) in [(&original_binary, &binary), (&original_worker, &worker)] {
            fs::copy(from, to).unwrap();
            fs::set_permissions(to, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let ready = temporary.path().join("ready");
        let release = temporary.path().join("release");
        let output = temporary.path().join("output");
        let mut options = options(binary.clone(), output.clone());
        options.root = copied.clone();
        options.case = Some("no_keys_overrides_saved".into());
        options.runner = vec![
            "/bin/sh".into(),
            original::fixture("after-probe-runner.sh").into_os_string(),
        ];
        options.environment.insert(
            "REGISTRATION_TEST_READY".into(),
            ready.clone().into_os_string(),
        );
        options.environment.insert(
            "REGISTRATION_TEST_RELEASE".into(),
            release.clone().into_os_string(),
        );
        let handle = std::thread::spawn(move || {
            native::run(&options, &worker)
                .map(|_| ())
                .map_err(|e| e.to_string())
        });
        let deadline = Instant::now() + Duration::from_secs(20);
        while !ready.is_file() && !handle.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        let reached = ready.is_file();
        if reached {
            if mutation == "source" {
                fs::write(
                    copied.join("rust/tools/registration_source.py"),
                    b"changed after source/probe completed\n",
                )
                .unwrap();
            } else {
                let replacement = temporary.path().join("replacement");
                fs::copy("/bin/false", &replacement).unwrap();
                fs::rename(replacement, temporary.path().join(mutation)).unwrap();
            }
        }
        // Always release on a failed readiness assertion, so the test never
        // leaves its actual subprocess blocked behind a test-only barrier.
        fs::write(release, b"release\n").unwrap();
        let result = handle.join().unwrap();
        assert!(
            reached,
            "{mutation}: actual probe did not complete before mutation: {result:?}"
        );
        assert!(
            result.is_err(),
            "{mutation}: results from the old identity were published under the replacement"
        );
        assert!(
            !output.join("result.json").exists(),
            "{mutation}: stale success publication"
        );
    }
}

#[test]
fn actual_cli_delayed_collector_preserves_both_sides_headers_and_event_order() {
    use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
    use std::time::Duration;
    let temporary = tempfile::tempdir().unwrap();
    let output = temporary.path().join("actual-delayed-cli");
    let mut spec = CommandSpec::new_os(
        vec![
            worker().into_os_string(),
            probe().into_os_string(),
            output.clone().into_os_string(),
            "--case".into(),
            "clock_year_ten_thousand".into(),
        ],
        Duration::from_secs(200),
    );
    let mut environment = std::env::vars_os().collect::<BTreeMap<_, _>>();
    environment.insert("OPENPILOT_RUST_ROOT".into(), root().into_os_string());
    environment.insert("REGISTRATION_COLLECTOR_DELAY_MS".into(), "800".into());
    spec.set_os_env(environment);
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    let captured = NativeRunner.run(&spec).unwrap();
    assert_eq!(
        captured.code,
        0,
        "{}",
        String::from_utf8_lossy(&captured.stderr)
    );
    let result = read(&output.join("result.json"));
    assert_eq!(text(&field(&result, "result")), "PASS");
    let rows = array(&field(&result, "cases"));
    assert_eq!(rows.len(), 1);
    assert_eq!(text(&field(&rows[0], "case")), "clock_year_ten_thousand");
    assert_eq!(text(&field(&rows[0], "result")), "PASS");
    let case = output.join("clock_year_ten_thousand");
    let mut schedules = fs::read_dir(&case)
        .unwrap()
        .map(|entry| {
            entry
                .unwrap()
                .path()
                .join("collector/publisher-schedule.json")
        })
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    schedules.sort();
    let mut expected =
        ["source", "native"].map(|side| case.join(side).join("collector/publisher-schedule.json"));
    expected.sort();
    assert_eq!(
        schedules, expected,
        "exactly two real source/native schedules"
    );
    for path in schedules {
        let schedule: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(schedule["requested_delay_ms"], 800);
        let before = schedule["before_publisher_header"].as_array().unwrap();
        let reset = schedule["after_publisher_header"].as_array().unwrap();
        let refreshed = schedule["after_reader_refresh_header"].as_array().unwrap();
        for header in [before, reset, refreshed] {
            assert_eq!(header.len(), 3 + 3 * 40, "pinned actual msgq header ABI");
        }
        assert!(before[0].as_u64().unwrap() >= 2);
        assert_eq!(before[2], 0);
        assert!(before[83..]
            .iter()
            .any(|value| value.as_u64().unwrap() != 0));
        assert_eq!(reset[0], 0);
        assert_ne!(reset[2], 0);
        assert!(reset[83..].iter().all(|value| value.as_u64().unwrap() == 0));
        assert!(refreshed[0].as_u64().unwrap() > 0);
        assert_eq!(refreshed[2], reset[2]);
        assert!(refreshed[83..]
            .iter()
            .any(|value| value.as_u64().unwrap() != 0));
        let subscribed = schedule["subscriber_registered_seconds"].as_f64().unwrap();
        let released = schedule["release_seconds"].as_f64().unwrap();
        let acknowledged = schedule["publisher_ack_seconds"].as_f64().unwrap();
        let refreshed_at = schedule["reader_refresh_seconds"].as_f64().unwrap();
        assert!(subscribed >= 0.0);
        assert!(released - subscribed >= 0.8);
        assert!(acknowledged >= released);
        assert!(refreshed_at >= acknowledged);
    }
}
