//! Synthetic protocol fixtures only; functional expectations come from the independent original.
#[allow(dead_code)]
#[path = "../src/controlsd_check.rs"]
mod controller;
use controller::{numeric, reference};
use serde_json::json;

#[test]
fn original_print_filter_preserves_every_other_byte_and_native_prints() {
    let input = "DBC: a\r\nUsing Hyundai CAN FD checksum\nUsing Hyundai CAN FD checksum\r\n DBC: a\nDBC:a\nkeep\u{85}DBC: b\u{2028}last";
    assert_eq!(
        reference::strip_original_prints(input),
        "Using Hyundai CAN FD checksum\r\n DBC: a\nDBC:a\nkeep\u{85}last"
    );
    assert_eq!(
        reference::strip_original_prints("Using Hyundai CAN FD checksum"),
        "Using Hyundai CAN FD checksum"
    );
    assert_eq!(reference::strip_original_prints("DBC: unterminated"), "");
    assert_eq!(
        reference::strip_original_prints("x\rDBC: hidden\ry"),
        "x\ry"
    );
}
#[test]
fn ten_tokens_clone_case_three_without_changing_base_or_sharing_frames() {
    let mut cases = vec![
        json!({}),
        json!({}),
        json!({}),
        json!({"name":"base","params":{"other":[9]},"frames":[0,1,2,3,4,5]}),
    ];
    let original = cases.clone();
    controller::extend_fingerprint_cases(&mut cases).unwrap();
    assert_eq!(&cases[..4], original);
    assert_eq!(cases.len(), 14);
    for (row, token) in cases[4..].iter().zip(controller::TOKENS) {
        assert_eq!(row["name"], format!("fingerprints-valid-{token}"));
        assert_eq!(row["frames"], json!([0, 1, 2, 3, 4]));
        assert_eq!(row["params"]["other"], json!([9]));
        let raw: Vec<u8> = serde_json::from_value(row["params"]["FingerPrints"].clone()).unwrap();
        assert_eq!(
            String::from_utf8(raw).unwrap(),
            format!("{{0: {{{token}: 8}}, 1: {{}}, 2: {{}}, 3: {{}}}}")
        );
    }
    cases[4]["frames"][0] = json!("changed");
    assert_eq!(cases[3], original[3]);
    assert_eq!(cases[5]["frames"][0], 0);
}
fn packet(time: u64) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<openpilot_cereal::log_capnp::event::Builder<'_>>();
    event.set_log_mono_time(time);
    event.set_valid(true);
    event.init_controls_state().set_curvature(0.125);
    capnp::serialize::write_message_to_words(&message)
}
#[test]
fn full_event_defaults_order_headers_and_nonfinite_json_survive_adapter() {
    let mut raw = json!([{"name":"contract","printed":"","rows":[{"state":{"x":0.0},"packets":[packet(7),packet(8)]}]}]);
    reference::normalize(&mut raw).unwrap();
    let pubs = &raw[0]["rows"][0]["publications"];
    assert_eq!(pubs[0]["service"], "controlsState");
    assert_eq!(pubs[0]["event"]["logMonoTime"], 7);
    assert_eq!(pubs[1]["event"]["logMonoTime"], 8);
    assert_eq!(pubs[0]["event"]["valid"], true);
    assert!(pubs[0]["event"]["controlsState"].as_object().unwrap().len() > 10);
    assert!(raw[0]["rows"][0].get("packets").is_none());
    let expected = raw.clone();
    let (_, result) = reference::compare(&expected, raw.clone()).unwrap();
    assert_eq!(result["cases"], 1);
    let mut reordered = raw.clone();
    reordered[0]["rows"][0]["publications"]
        .as_array_mut()
        .unwrap()
        .reverse();
    assert!(reference::compare(&expected, reordered).is_err());
    let mut printed = raw.clone();
    printed[0]["printed"] = json!("DBC: native must stay\n");
    assert!(reference::compare(&expected, printed).is_err());
    let nonfinite = numeric::parse_python(br#"[NaN,Infinity,-Infinity,"NaN",-0.0]"#).unwrap();
    let encoded = reference::python_json(&nonfinite).unwrap();
    assert_eq!(numeric::parse_python(&encoded).unwrap(), nonfinite);
    let mut trailing = packet(7);
    trailing.extend_from_slice(&packet(8));
    let mut bad = json!([{"rows":[{"packets":[trailing]}]}]);
    assert!(reference::normalize(&mut bad).is_err());
}
#[test]
fn original_only_print_filter_and_shared_tolerance_remain_path_sensitive() {
    let expected = json!([{"printed":"DBC: source\nkeep\n","rows":[{"state":1.0,"publications":[{"service":"x","event":{"value":1.0}}]}]}]);
    let actual = json!([{"printed":"keep\n","rows":[{"state":1.0,"publications":[{"service":"x","event":{"value":1.000001}}]}]}]);
    reference::compare(&expected, actual.clone()).unwrap();
    let mut bad = actual;
    bad[0]["rows"][0]["state"] = json!(1.000001);
    assert!(reference::compare(&expected, bad).is_err());
}
#[test]
fn cli_rejects_missing_duplicate_and_unknown_options() {
    for args in [
        vec![],
        vec!["--unknown", "x"],
        vec!["--root"],
        vec!["--root", "a", "--root", "b"],
    ] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_check-controlsd-native"))
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
}

#[test]
#[ignore = "requires the admitted real original 40-case capture in Actions"]
fn original_admitted_owned_process_failures_and_foreign_receipts_never_pass() {
    use openpilot_dev_tools::{
        qa_process::{Captured, CommandSpec, Runner},
        seal::Result,
    };
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicBool, Ordering},
    };
    struct Failing<'a> {
        stop: &'a AtomicBool,
        mode: u8,
        calls: usize,
    }
    impl Runner for Failing<'_> {
        fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
            self.calls += 1;
            assert_eq!(spec.timeout, std::time::Duration::from_secs(600));
            assert_eq!(spec.env["PATH"], "/nonexistent");
            assert!(spec.env.contains_key("CONTROLS_NUMERICS"));
            assert!(spec.input.as_ref().is_some_and(|s| !s.is_empty()));
            assert!(spec.effective_argv()[1]
                .to_string_lossy()
                .contains("owned-trace-"));
            if self.mode == 1 {
                self.stop.store(true, Ordering::SeqCst);
            }
            if self.mode == 2 {
                fs::write(PathBuf::from(&spec.effective_argv()[1]), b"[]")?;
            }
            Ok(Captured {
                code: if self.mode == 2 { 0 } else { 17 },
                stdout: b"out".to_vec(),
                stderr: b"err".to_vec(),
            })
        }
    }
    let env = |name| std::env::var_os(name).expect("Actions capture variable");
    let original = PathBuf::from(env("CONTROLSD_CONTROLLER_ORIGINAL"));
    let capture_sha = std::env::var("CONTROLSD_CONTROLLER_CAPTURE_SHA256").unwrap();
    let limits = rustix::process::getrlimit(rustix::process::Resource::Fsize);
    rustix::process::setrlimit(
        rustix::process::Resource::Fsize,
        rustix::process::Rlimit {
            current: Some(512 * 1024 * 1024),
            maximum: limits.maximum,
        },
    )
    .unwrap();
    let temp = tempfile::tempdir().unwrap();
    for mode in 0..5 {
        let stop = AtomicBool::new(mode == 3);
        let evidence = temp.path().join(format!("evidence-{mode}"));
        if mode == 4 {
            fs::create_dir(&evidence).unwrap();
            fs::write(evidence.join("results.json"), b"foreign").unwrap();
        }
        let options = controller::Options {
            root: env("CONTROLSD_CONTROLLER_ROOT").into(),
            binary: env("CONTROLSD_CONTROLLER_TRACE").into(),
            original: original.clone(),
            numerics: env("CONTROLS_NUMERICS").into(),
            python_prefix: env("CONTROLSD_CONTROLLER_PYTHON_PREFIX").into(),
            evidence: evidence.clone(),
            source_commit: std::env::var("CONTROLSD_CONTROLLER_SOURCE_SHA").unwrap(),
            capture_sha256: capture_sha.clone(),
        };
        let mut runner = Failing {
            stop: &stop,
            mode,
            calls: 0,
        };
        assert!(controller::execute(&options, &mut runner, &stop).is_err());
        assert_eq!(runner.calls, usize::from(mode < 3));
        if mode == 4 {
            assert_eq!(fs::read(evidence.join("results.json")).unwrap(), b"foreign");
        } else {
            assert!(!evidence.join("results.json").exists());
        }
    }
}

#[test]
fn owned_slow_child() {
    if let Some(path) = std::env::var_os("CONTROLSD_OWNED_CHILD_PID") {
        std::fs::write(path, std::process::id().to_string()).unwrap();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
}
#[test]
fn controller_owned_runner_forwards_cancel_and_deadline_and_reaps_child() {
    use openpilot_dev_tools::qa_process::CommandSpec;
    use std::{
        fs,
        path::Path,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::{Duration, Instant},
    };
    for cancel in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let pidfile = temp.path().join("pid");
        let mut spec = CommandSpec::new_os(
            vec![
                std::env::current_exe().unwrap().into_os_string(),
                "--exact".into(),
                "owned_slow_child".into(),
                "--nocapture".into(),
            ],
            Duration::from_secs(if cancel { 10 } else { 2 }),
        );
        spec.env.insert(
            "CONTROLSD_OWNED_CHILD_PID".into(),
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
            assert!(path.exists());
            if cancel {
                flag.store(true, Ordering::SeqCst);
            }
        });
        assert!(controller::run_owned(&spec, &stop).is_err());
        observer.join().unwrap();
        let pid = fs::read_to_string(pidfile).unwrap();
        assert!(!Path::new("/proc").join(pid).exists());
    }
}
