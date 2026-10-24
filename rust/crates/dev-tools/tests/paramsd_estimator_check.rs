//! Synthetic values exercise comparison/process contracts only. The independent
//! functional expectation is captured from the retained original in Actions.
#[allow(dead_code)]
#[path = "../src/paramsd_estimator_check.rs"]
mod controller;
use controller::{numeric as n, reference as r};
use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[test]
fn asymmetric_relative_reference_and_nonfinite_contract() {
    // Large tolerance intentionally makes the asymmetric window observable
    // without relying on a particular near-ULP decimal representation.
    assert!(n::is_close(10., 11.2, 0.1));
    assert!(!n::is_close(11.2, 10., 0.1));
    assert!(n::is_close(f64::NAN, f64::NAN, 1e-9));
    assert!(n::is_close(f64::INFINITY, f64::INFINITY, 1e-9));
    assert!(n::is_close(f64::NEG_INFINITY, f64::NEG_INFINITY, 1e-9));
    assert!(!n::is_close(f64::INFINITY, f64::NEG_INFINITY, 1e-9));
    assert!(!n::is_close(1., f64::INFINITY, 1e-9));
    assert!(!n::is_close(f64::NAN, 1., 1e-9));
    assert!(n::is_close(-0., 0., 1e-9));
}
#[test]
fn complete_paths_keys_lists_logs_clock_and_python_scalar_equality() {
    let original = json!({"x":[1.], "event":{"logMonoTime":1234567890u64,"liveParameters":{"steerRatio":1.,"debugFilterState":{"value":[1.]}}},"logs":[["warning","exact NaN text"]], "active":true,"time":null});
    let mut actual = original.clone();
    actual["event"]["liveParameters"]["steerRatio"] = json!(1.000001);
    let mut stats = n::Stats::default();
    n::compare(&original, &actual, "paramsd", &mut stats).unwrap();
    assert_eq!(stats.numeric_comparisons, 3);
    assert!(stats.max_absolute_error > 0.);
    for path in ["x", "debug", "clock", "log", "extra", "list", "null"] {
        let mut bad = actual.clone();
        match path {
            "x" => bad["x"][0] = json!(1.000001),
            "debug" => {
                bad["event"]["liveParameters"]["debugFilterState"]["value"][0] = json!(1.000001)
            }
            "clock" => bad["event"]["logMonoTime"] = json!(1234567891u64),
            "log" => bad["logs"][0][1] = json!("changed"),
            "extra" => bad["event"]["unexpected"] = json!(false),
            "list" => bad["x"] = json!([1., 1.]),
            _ => bad["time"] = json!(0),
        }
        assert!(
            n::compare(&original, &bad, "paramsd", &mut n::Stats::default()).is_err(),
            "{path}"
        );
    }
    for (left, right) in [
        (json!(true), json!(1.)),
        (json!(1), json!(true)),
        (json!(9007199254740992u64), json!(9007199254740992.)),
    ] {
        n::compare(&left, &right, "scalar", &mut n::Stats::default()).unwrap();
    }
    assert!(n::compare(
        &json!(9007199254740993u64),
        &json!(9007199254740992.),
        "scalar",
        &mut n::Stats::default()
    )
    .is_err());
}
#[test]
fn nonfinite_json_is_typed_and_never_confused_with_log_strings() {
    let parsed =
        n::parse_python(br#"[NaN, Infinity, -Infinity, "NaN", "Infinity", -0.0]"#).unwrap();
    assert!(n::number(&parsed[0]).unwrap().is_nan());
    assert_eq!(n::number(&parsed[1]), Some(f64::INFINITY));
    assert_eq!(n::number(&parsed[2]), Some(f64::NEG_INFINITY));
    assert_eq!(parsed[3], "NaN");
    assert_eq!(parsed[4], "Infinity");
    for bad in [
        br#"[xNaN]"#.as_slice(),
        br#"[NaN0]"#,
        br#"["__paramsd_estimator_nan_f64__"]"#,
        br#"["\u005f_paramsd_estimator_nan_f64__"]"#,
    ] {
        assert!(n::parse_python(bad).is_err());
    }
    let mut stats = n::Stats::default();
    n::compare(&parsed, &parsed, "paramsd", &mut stats).unwrap();
    assert_eq!(stats.numeric_comparisons, 1);
}
fn packet() -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<openpilot_cereal::log_capnp::event::Builder<'_>>();
    event.set_log_mono_time(1234567890);
    event.set_valid(true);
    let mut params = event.init_live_parameters();
    params.set_steer_ratio(12.5);
    let mut debug = params.init_debug_filter_state();
    debug.reborrow().init_value(1).set(0, f64::NAN);
    debug.init_std(1).set(0, 2.);
    capnp::serialize::write_message_to_words(&message)
}
#[test]
fn event_schema_defaults_clock_debug_nonfinite_and_framing_are_preserved() {
    let bytes = packet();
    let event = r::event_dict(&bytes).unwrap();
    assert_eq!(event["logMonoTime"], 1234567890u64);
    assert_eq!(event["valid"], true);
    assert_eq!(event["liveParameters"]["steerRatio"], 12.5);
    assert!(
        n::number(&event["liveParameters"]["debugFilterState"]["value"][0])
            .unwrap()
            .is_nan()
    );
    assert_eq!(
        event["liveParameters"]["debugFilterState"]["std"],
        json!([2.])
    );
    assert!(event["liveParameters"]
        .as_object()
        .unwrap()
        .contains_key("sensorValid"));
    let mut extra = bytes.clone();
    extra.extend_from_slice(&bytes);
    assert!(r::event_dict(&extra).is_err());
    assert!(r::event_dict(&bytes[..bytes.len() - 1]).is_err());
}
fn raw_rows() -> Value {
    Value::Array(r::NAMES.iter().zip(r::COUNTS).map(|(name,count)|json!({"name":name,"rows":vec![json!({"x_bits":vec![(-0.0f64).to_bits();9],"p_bits":vec![f64::INFINITY.to_bits();81],"observed_bits":[f64::NAN.to_bits(),1.0f64.to_bits(),0],"packet":packet(),"logs":[],"active":false,"time":null});count]})).collect())
}
#[test]
fn wire_bits_dimensions_predecoded_fields_and_roster_fail_closed() {
    let raw = raw_rows();
    let mut decoded = raw.clone();
    r::normalize(&mut decoded).unwrap();
    assert_eq!(
        n::number(&decoded[0]["rows"][0]["x"][0]).unwrap().to_bits(),
        (-0.0f64).to_bits()
    );
    assert_eq!(
        n::number(&decoded[0]["rows"][0]["p"][0]),
        Some(f64::INFINITY)
    );
    assert!(n::number(&decoded[0]["rows"][0]["observed"][0])
        .unwrap()
        .is_nan());
    for kind in 0..5 {
        let mut bad = raw.clone();
        match kind {
            0 => bad[0]["rows"][0]["x_bits"] = json!([0]),
            1 => bad[0]["rows"][0]["p_bits"][0] = json!(-1),
            2 => bad[0]["rows"][0]["event"] = json!({}),
            3 => bad[0]["name"] = json!("reordered"),
            _ => bad[0]["rows"][0]["x"] = json!([]),
        }
        assert!(r::normalize(&mut bad).is_err(), "{kind}");
    }
}

struct Fixture {
    _temp: tempfile::TempDir,
    options: controller::Options,
}
fn copy(root: &Path, source: &Path, name: &str) {
    let out = root.join(name);
    fs::create_dir_all(out.parent().unwrap()).unwrap();
    fs::copy(source.join(name), out).unwrap();
}
fn hash(path: &Path) -> String {
    format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}
fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    fs::create_dir(&root).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let mut names = vec![
        "rust/tools/check_paramsd.py",
        "rust/tools/paramsd_source.py",
        "rust/tools/paramsd_fixture.py",
        "rust/crates/dev-tools/src/paramsd_fixture_generate.rs",
    ];
    // Keep this synthetic admission minimum synchronized with production.
    names.extend(controller::required_source_for_test());
    names.extend([
        "rust/crates/dev-tools/src/paramsd_estimator_check.rs",
        "rust/crates/dev-tools/src/paramsd_estimator_check/identity.rs",
        "rust/crates/dev-tools/src/paramsd_estimator_check/numeric.rs",
        "rust/crates/dev-tools/src/paramsd_estimator_check/reference.rs",
        "rust/crates/dev-tools/src/bin/check_paramsd_estimator_native.rs",
    ]);
    for name in &names {
        copy(&root, source, name);
    }
    let original = temp.path().join("original");
    let oracle = temp.path().join("oracle");
    fs::create_dir(&original).unwrap();
    fs::create_dir(&oracle).unwrap();
    // Inert placeholders are ONLY for failing process/admission contract tests.
    // They are never executed and no synthetic trace receives a passing receipt.
    for name in ["car.cpp", "car.h", "libcar.so", "ekf_sym_pyx.contract.so"] {
        fs::write(oracle.join(name), b"inert contract provider").unwrap();
    }
    let binary = temp.path().join("native-test-elf");
    fs::copy(std::env::current_exe().unwrap(), &binary).unwrap();
    let input = openpilot_dev_tools::paramsd_fixture_generate::scenarios();
    let rows = Value::Array(
        r::NAMES
            .iter()
            .zip(r::COUNTS)
            .map(|(name, count)| json!({"name":name,"rows":vec![Value::Null;count]}))
            .collect(),
    );
    let mut members = serde_json::Map::new();
    for (name, value) in [
        ("input.json", input),
        ("source.json", rows),
        ("results.json", json!({"pass":true,"cases":7,"steps":1196})),
    ] {
        fs::write(original.join(name), serde_json::to_vec(&value).unwrap()).unwrap();
        members.insert(name.into(), json!(hash(&original.join(name))));
    }
    names.sort_unstable();
    names.dedup();
    let mut paths: Vec<PathBuf> = names.iter().map(|name| root.join(name)).collect();
    paths.extend(
        ["car.cpp", "car.h", "libcar.so", "ekf_sym_pyx.contract.so"].map(|name| oracle.join(name)),
    );
    let bindings: Vec<_> = paths
        .iter()
        .map(|p| json!({"path":p,"sha256":hash(p)}))
        .collect();
    let commit = "a".repeat(40);
    fs::write(original.join("capture.json"),serde_json::to_vec(&json!({"schema":1,"source_commit":commit,"root":root,"oracle":oracle,"binary":{"path":binary,"sha256":hash(&binary)},"members":members,"bindings":bindings})).unwrap()).unwrap();
    let evidence = temp.path().join("evidence");
    Fixture {
        _temp: temp,
        options: controller::Options {
            root,
            binary,
            original,
            oracle,
            evidence,
            source_commit: commit,
        },
    }
}
struct FailRunner<'a> {
    calls: usize,
    stop: Option<&'a AtomicBool>,
    mutate: Option<PathBuf>,
    code: i32,
    error: bool,
}
impl Runner for FailRunner<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        assert_eq!(spec.timeout, std::time::Duration::from_secs(180));
        assert_eq!(spec.env["PATH"], "/nonexistent");
        assert!(spec.input.as_ref().is_some_and(|v| !v.is_empty()));
        if let Some(stop) = self.stop {
            stop.store(true, Ordering::SeqCst);
        }
        if let Some(path) = &self.mutate {
            fs::write(path, b"changed source").unwrap();
        }
        if self.error {
            return Err("injected runner failure".into());
        }
        Ok(Captured {
            code: self.code,
            stdout: b"child stdout".to_vec(),
            stderr: b"child stderr".to_vec(),
        })
    }
}
#[test]
fn process_exit_error_cancellation_source_changes_and_foreign_evidence_never_pass() {
    for mode in 0..6 {
        let fixture = fixture();
        let stop = AtomicBool::new(mode == 3);
        let mut runner = FailRunner {
            calls: 0,
            stop: if mode == 2 { Some(&stop) } else { None },
            mutate: if mode == 4 {
                Some(fixture.options.root.join("rust/tools/check_paramsd.py"))
            } else {
                None
            },
            code: 17,
            error: mode == 1,
        };
        if mode == 5 {
            fs::create_dir(&fixture.options.evidence).unwrap();
            fs::write(
                fixture.options.evidence.join("results.json"),
                b"foreign receipt",
            )
            .unwrap();
        }
        let mut output = Vec::new();
        let error = controller::execute(&fixture.options, &mut runner, &stop, &mut output)
            .unwrap_err()
            .to_string();
        assert!(output.is_empty());
        assert_eq!(runner.calls, usize::from(mode != 3 && mode != 5));
        if mode == 5 {
            assert_eq!(
                fs::read(fixture.options.evidence.join("results.json")).unwrap(),
                b"foreign receipt"
            );
        } else {
            assert!(!fixture.options.evidence.join("results.json").exists());
        }
        if mode == 0 {
            assert!(error.contains("exited 17"), "{error}");
        }
        if mode == 1 {
            assert!(error.contains("injected runner failure"), "{error}");
        }
        if mode == 2 || mode == 3 {
            assert!(error.contains("cancelled"), "{error}");
        }
        if mode == 4 {
            assert!(error.contains("changed"), "{error}");
        }
    }
}

#[test]
#[ignore = "fresh independent NumPy observations are supplied only in Actions"]
fn matches_fresh_numpy_comparison_boundary_observations() {
    let path = std::env::var_os("PARAMSD_NUMPY_CONTROLS").expect("Actions NumPy control corpus");
    let corpus: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(corpus["numpy_version"], "2.5.3");
    let cases = corpus["cases"].as_array().unwrap();
    assert!(cases.len() > 1000);
    for case in cases {
        let left = f64::from_bits(case["expected_bits"].as_u64().unwrap());
        let right = f64::from_bits(case["actual_bits"].as_u64().unwrap());
        let tolerance = case["tolerance"].as_f64().unwrap();
        assert_eq!(
            n::is_close(left, right, tolerance),
            case["close"].as_bool().unwrap(),
            "{case}"
        );
    }
}

#[test]
fn owned_slow_child() {
    if let Some(path) = std::env::var_os("PARAMSD_OWNED_CHILD_PID") {
        fs::write(path, std::process::id().to_string()).unwrap();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
}
#[test]
fn real_owned_process_deadline_and_cancellation_reap_direct_child() {
    use openpilot_dev_tools::qa_process::NativeRunner;
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };
    for cancelled in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let pidfile = temp.path().join("pid");
        let mut spec = CommandSpec::new_os(
            vec![
                std::env::current_exe().unwrap().into_os_string(),
                "--exact".into(),
                "owned_slow_child".into(),
                "--nocapture".into(),
            ],
            Duration::from_secs(10),
        );
        spec.env.insert(
            "PARAMSD_OWNED_CHILD_PID".into(),
            pidfile.to_str().unwrap().into(),
        );
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let path = pidfile.clone();
        let observer = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !path.exists() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(2));
            }
            assert!(path.exists(), "real child never started");
            if cancelled {
                flag.store(true, Ordering::SeqCst);
            }
        });
        let deadline = Instant::now() + Duration::from_secs(if cancelled { 10 } else { 2 });
        assert!(NativeRunner.run_controlled(&spec, deadline, &stop).is_err());
        observer.join().unwrap();
        let pid = fs::read_to_string(pidfile).unwrap();
        assert!(
            !Path::new("/proc").join(pid).exists(),
            "direct child was not reaped"
        );
    }
}

#[test]
fn cli_rejects_unknown_duplicate_and_missing_options() {
    for args in [
        vec![],
        vec!["--unknown", "value"],
        vec!["--root", "a", "--root", "b"],
        vec!["--root"],
    ] {
        let output =
            std::process::Command::new(env!("CARGO_BIN_EXE_check-paramsd-estimator-native"))
                .args(args)
                .output()
                .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8(output.stderr)
            .unwrap()
            .contains("paramsd estimator native:"));
    }
}

#[test]
fn changed_original_member_provider_or_elf_never_invokes_child() {
    for mode in 0..3 {
        let fixture = fixture();
        match mode {
            0 => fs::write(fixture.options.original.join("source.json"), b"[]").unwrap(),
            1 => fs::write(
                fixture.options.oracle.join("libcar.so"),
                b"different provider",
            )
            .unwrap(),
            _ => {
                fs::write(&fixture.options.binary, vec![0; 64]).unwrap();
                let path = fixture.options.original.join("capture.json");
                let mut capture: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                capture["binary"]["sha256"] = json!(hash(&fixture.options.binary));
                fs::write(path, serde_json::to_vec(&capture).unwrap()).unwrap();
            }
        }
        let mut runner = FailRunner {
            calls: 0,
            stop: None,
            mutate: None,
            code: 17,
            error: false,
        };
        assert!(controller::execute(
            &fixture.options,
            &mut runner,
            &AtomicBool::new(false),
            &mut Vec::new()
        )
        .is_err());
        assert_eq!(runner.calls, 0);
        assert!(!fixture.options.evidence.exists());
    }
}
