//! Mandatory cloud pair runs the retained original, never captured candidate answers.
#![cfg(all(target_os = "linux", feature = "pandad-safety-padding-native"))]
use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn required(name: &str) -> PathBuf {
    let path = PathBuf::from(
        std::env::var_os(name)
            .unwrap_or_else(|| panic!("mandatory cloud prerequisite missing: {name}")),
    );
    fs::canonicalize(path).unwrap()
}
fn workspace() -> tempfile::TempDir {
    let path = required("PANDAD_PADDING_TEST_OUTPUT");
    tempfile::Builder::new()
        .prefix("pair-")
        .tempdir_in(path)
        .unwrap()
}
fn checker() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_check-pandad-safety-padding"))
}
fn retired_admission_root(pair: &Path) -> PathBuf {
    let destination = pair.join("native-admission-root");
    let contract: Value = json(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/pandad-safety-padding/contract.json"),
    );
    for name in contract["source_sha256"].as_object().unwrap().keys() {
        let target = destination.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(root().join(name), target).unwrap();
    }
    assert!(!destination
        .join("rust/tools/check_pandad_safety_padding.py")
        .exists());
    destination
}
fn json(path: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn hash(path: &Path) -> String {
    format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}
fn command(executable: &Path, args: Vec<OsString>, admission: &Path) -> CommandSpec {
    let mut argv = vec![executable.into()];
    argv.extend(args);
    let mut spec = CommandSpec::new_os(argv, Duration::from_secs(300));
    let mut env = spec.effective_env();
    env.insert("OPENPILOT_RUST_ROOT".into(), admission.into());
    env.insert(
        "PYTHONPATH".into(),
        std::env::join_paths([root(), root().join("rust/tools")]).unwrap(),
    );
    spec.set_os_env(env);
    spec
}
fn invoke(spec: &CommandSpec) -> openpilot_dev_tools::qa_process::Captured {
    let got = NativeRunner.run(spec).unwrap();
    assert_eq!(got.code, 0, "{}", String::from_utf8_lossy(&got.stderr));
    got
}
fn rows(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
#[test]
fn unchanged_original_and_native_controllers_match_the_same_live_probes_and_all_transcripts() {
    let temp = workspace();
    // Keep all evidence, including failure transcripts, for Actions upload.
    let pair = temp.keep();
    let admission = retired_admission_root(&pair);
    let source = required("PANDAD_PADDING_SOURCE_BINARY");
    let binary = required("PANDAD_PADDING_NATIVE_BINARY");
    let python = required("PANDAD_PADDING_PYTHON");
    let source_before = hash(&source);
    let binary_before = hash(&binary);
    let original = pair.join("original");
    let native_present = pair.join("native-present");
    let native_absent = pair.join("native-absent");
    let common = |output: &Path| {
        vec![
            "--source".into(),
            source.clone().into(),
            "--binary".into(),
            binary.clone().into(),
            "--output".into(),
            output.into(),
        ]
    };
    let mut args = vec![
        "-P".into(),
        root()
            .join("rust/tools/check_pandad_safety_padding.py")
            .into(),
    ];
    args.extend(common(&original));
    let expected = invoke(&command(&python, args, &root()));
    fs::write(pair.join("original.stdout"), &expected.stdout).unwrap();
    fs::write(pair.join("original.stderr"), &expected.stderr).unwrap();
    let native_run = |name: &str, output: &Path, admitted: &Path| {
        let mut spec = command(&checker(), common(output), admitted);
        let mut environment = spec.effective_env();
        environment.insert("PATH".into(), "/nonexistent".into());
        spec.set_os_env(environment);
        let got = invoke(&spec);
        fs::write(pair.join(format!("{name}.stdout")), &got.stdout).unwrap();
        fs::write(pair.join(format!("{name}.stderr")), &got.stderr).unwrap();
        got
    };
    let present = native_run("native-present", &native_present, &root());
    let absent = native_run("native-absent", &native_absent, &admission);
    assert_eq!(
        present.stdout, absent.stdout,
        "same native checker stdout drift"
    );
    assert_eq!(
        present.stderr, absent.stderr,
        "same native checker stderr drift"
    );
    let expected_report = json(original.join("report.json"));
    let captured = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pandad-safety-padding/edb42bf0-original");
    assert_eq!(expected_report, json(captured.join("report.json")));
    for name in [
        "car-params.stdout",
        "car-params.stderr",
        "source.stdout",
        "source.stderr",
        "native.stdout",
        "native.stderr",
    ] {
        assert_eq!(
            fs::read(original.join(name)).unwrap(),
            fs::read(captured.join(name)).unwrap(),
            "fresh original diverges from sealed edb42bf0 capture: {name}"
        );
    }
    assert_eq!(
        expected_report,
        serde_json::from_slice::<Value>(&expected.stdout).unwrap()
    );
    for (name, output, got, historical_present) in [
        ("native-present", &native_present, &present, true),
        ("native-absent", &native_absent, &absent, false),
    ] {
        assert_eq!(expected_report, json(output.join("report.json")), "{name}");
        assert_eq!(
            expected_report,
            serde_json::from_slice::<Value>(&got.stdout).unwrap()
        );
        for label in ["car-params", "source", "native"] {
            // Responses and diagnostics stay byte-identical for both admission states.
            for suffix in ["stdout", "stderr"] {
                let file = format!("{label}.{suffix}");
                assert_eq!(
                    fs::read(original.join(&file)).unwrap(),
                    fs::read(output.join(&file)).unwrap(),
                    "{name}: {file}"
                );
            }
            let file = format!("{label}.input.jsonl");
            let mut expected_rows = rows(&original.join(&file));
            let mut actual_rows = rows(&output.join(&file));
            assert_eq!(
                expected_rows.len(),
                if label == "car-params" { 1 } else { 4 }
            );
            assert_eq!(expected_rows.len(), actual_rows.len());
            if label != "car-params" {
                let child = if label == "source" {
                    "source-params"
                } else {
                    "native-params"
                };
                for (owned, rows) in [(&original, &mut expected_rows), (output, &mut actual_rows)] {
                    for row in rows {
                        let params = row.as_object_mut().unwrap().remove("params_root").unwrap();
                        assert_eq!(
                            params.as_str().unwrap(),
                            owned.join(child).to_str().unwrap()
                        );
                    }
                }
            }
            assert_eq!(expected_rows, actual_rows, "{name}: {file}");
        }
        let provenance = json(output.join("provenance.json"));
        assert_eq!(provenance["executables"]["source"], source_before);
        assert_eq!(provenance["executables"]["native"], binary_before);
        let expected_pins = json(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/pandad-safety-padding/contract.json"),
        );
        let admitted = provenance["admitted_source_sha256"].as_object().unwrap();
        assert_eq!(admitted.len(), 6 + usize::from(historical_present));
        for (source_name, digest) in expected_pins["source_sha256"].as_object().unwrap() {
            assert_eq!(
                admitted.get(source_name).unwrap(),
                digest,
                "{name}: {source_name}"
            );
        }
        let historical =
            provenance["admitted_source_sha256"].get("rust/tools/check_pandad_safety_padding.py");
        assert_eq!(historical.is_some(), historical_present, "{name} admission");
        if let Some(digest) = historical {
            assert_eq!(
                digest,
                "f0d0867a9481a698df8da04d472bdf2e2d6aa77da2a3ae29fba08aae74d5a836"
            );
        }
    }
    assert_eq!(hash(&source), source_before);
    assert_eq!(hash(&binary), binary_before);
    // Bind the existing native source builder's complete declared input map,
    // not just the controller's narrower direct-policy admission pins.
    let build = json(required("PANDAD_PADDING_SOURCE_MANIFEST"));
    assert_eq!(build["binary_sha256"], source_before);
    assert_eq!(Path::new(build["binary"].as_str().unwrap()), source);
    let captured_build =
        include_bytes!("fixtures/pandad-safety-padding/edb42bf0-source-build-manifest.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(captured_build)),
        "5590942fb8df47efe83cd9d34742439e195e1674a2180bca74cdd291c2e6dbb7"
    );
    let captured_build: Value = serde_json::from_slice(captured_build).unwrap();
    let inputs = build["sources"].as_object().unwrap();
    assert_eq!(inputs.len(), 21);
    assert_eq!(
        build["sources"], captured_build["sources"],
        "complete source build input closure drift"
    );
    assert!(inputs.contains_key("openpilot/selfdrive/pandad/panda_safety.cc"));
    assert!(inputs.contains_key("opendbc_repo/opendbc/car/car.capnp"));
    for (name, wanted) in inputs {
        assert_eq!(hash(&root().join(name)), wanted.as_str().unwrap(), "{name}");
    }
}
#[test]
fn actual_cli_statuses_and_missing_probe_artifact_phases_match_the_original() {
    let temp = workspace();
    let pair = temp.keep();
    let admission = retired_admission_root(&pair);
    let source = required("PANDAD_PADDING_SOURCE_BINARY");
    let binary = required("PANDAD_PADDING_NATIVE_BINARY");
    let python = required("PANDAD_PADDING_PYTHON");
    let native_checker = checker();
    for args in [
        vec![],
        vec![OsString::from("--unknown")],
        vec!["--source".into()],
        vec!["--help".into()],
    ] {
        let expected_code = if args == vec![OsString::from("--help")] {
            0
        } else {
            2
        };
        let native = NativeRunner
            .run(&command(&checker(), args.clone(), &admission))
            .unwrap();
        let mut original_args = vec![
            "-P".into(),
            root()
                .join("rust/tools/check_pandad_safety_padding.py")
                .into(),
        ];
        original_args.extend(args);
        let original = NativeRunner
            .run(&command(&python, original_args, &root()))
            .unwrap();
        assert_eq!(native.code, expected_code);
        assert_eq!(original.code, expected_code);
    }
    for missing_source in [true, false] {
        let absent = pair.join("absent-probe");
        let mut outputs = vec![];
        for original in [true, false] {
            let output = pair.join(format!("missing-{missing_source}-{original}"));
            let mut args = if original {
                vec![
                    OsString::from("-P"),
                    root()
                        .join("rust/tools/check_pandad_safety_padding.py")
                        .into(),
                ]
            } else {
                vec![]
            };
            args.extend([
                "--source".into(),
                if missing_source {
                    absent.clone()
                } else {
                    source.clone()
                }
                .into(),
                "--binary".into(),
                if missing_source {
                    binary.clone()
                } else {
                    absent.clone()
                }
                .into(),
                "--output".into(),
                output.clone().into(),
            ]);
            let original_root = root();
            let got = NativeRunner
                .run(&command(
                    if original { &python } else { &native_checker },
                    args,
                    if original { &original_root } else { &admission },
                ))
                .unwrap();
            assert_eq!(got.code, 1);
            assert!(!output.join("report.json").exists());
            outputs.push(output);
        }
        for name in [
            "car-params.input.jsonl",
            "car-params.stdout",
            "car-params.stderr",
            "source.input.jsonl",
            "source.stdout",
            "source.stderr",
            "native.input.jsonl",
            "native.stdout",
            "native.stderr",
        ] {
            assert_eq!(
                outputs[0].join(name).exists(),
                outputs[1].join(name).exists(),
                "{missing_source}: {name}"
            );
        }
    }
}
#[test]
fn sigint_reaps_an_acknowledged_owned_probe_without_publishing_success() {
    use rustix::process::{
        pidfd_open, pidfd_send_signal, waitid, Pid, PidfdFlags, Signal, WaitId, WaitIdOptions,
    };
    use std::{
        os::{fd::OwnedFd, unix::process::CommandExt},
        process::{Command, Stdio},
    };
    struct Owner {
        child: Option<std::process::Child>,
        id: OwnedFd,
        probe: Option<OwnedFd>,
    }
    impl Drop for Owner {
        fn drop(&mut self) {
            if let Some(probe) = &self.probe {
                let _ = pidfd_send_signal(probe, Signal::KILL);
            }
            if let Some(mut child) = self.child.take() {
                let _ = rustix::process::kill_process_group(Pid::from_child(&child), Signal::KILL);
                let _ = child.wait();
            }
        }
    }
    let temp = workspace();
    let path = temp.keep();
    let admission = retired_admission_root(&path);
    let output = path.join("interrupted");
    let marker = path.join("probe-pid");
    let blocked = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pandad-safety-padding/blocked-probe");
    let child = Command::new(checker())
        .args([
            "--source".into(),
            blocked.into_os_string(),
            "--binary".into(),
            required("PANDAD_PADDING_NATIVE_BINARY").into(),
            "--output".into(),
            output.clone().into(),
        ])
        .env("OPENPILOT_RUST_ROOT", admission)
        .env("PANDAD_PADDING_BLOCKED_PID", &marker)
        .stdout(Stdio::null())
        .stderr(fs::File::create(path.join("stderr")).unwrap())
        .process_group(0)
        .spawn()
        .unwrap();
    let id = pidfd_open(Pid::from_child(&child), PidfdFlags::empty()).unwrap();
    let mut owner = Owner {
        child: Some(child),
        id,
        probe: None,
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    let pid = loop {
        if let Ok(bytes) = fs::read_to_string(&marker) {
            if let Ok(pid) = bytes.trim().parse::<i32>() {
                break Pid::from_raw(pid).unwrap();
            }
        }
        assert!(
            Instant::now() < deadline,
            "probe startup acknowledgment absent"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    owner.probe = Some(pidfd_open(pid, PidfdFlags::empty()).unwrap());
    let probe_ready = || {
        rustix::event::poll(
            &mut [rustix::event::PollFd::new(
                owner.probe.as_ref().unwrap(),
                rustix::event::PollFlags::IN,
            )],
            Some(&rustix::event::Timespec::default()),
        )
        .unwrap()
    };
    assert_eq!(probe_ready(), 0, "acknowledged probe already exited");
    pidfd_send_signal(&owner.id, Signal::INT).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if waitid(
            WaitId::Pid(Pid::from_child(owner.child.as_ref().unwrap())),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        )
        .unwrap()
        .is_some()
        {
            break;
        }
        assert!(Instant::now() < deadline, "checker ignored cancellation");
        std::thread::sleep(Duration::from_millis(5));
    }
    let _ = rustix::process::kill_process_group(
        Pid::from_child(owner.child.as_ref().unwrap()),
        Signal::KILL,
    );
    assert_eq!(probe_ready(), 1, "owned probe survived cancellation");
    assert!(!owner.child.take().unwrap().wait().unwrap().success());
    assert!(!output.join("report.json").exists());
}
