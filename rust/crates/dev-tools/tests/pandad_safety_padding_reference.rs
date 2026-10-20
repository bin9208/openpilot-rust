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
fn json(path: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn hash(path: &Path) -> String {
    format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}
fn command(executable: &Path, args: Vec<OsString>) -> CommandSpec {
    let mut argv = vec![executable.into()];
    argv.extend(args);
    let mut spec = CommandSpec::new_os(argv, Duration::from_secs(300));
    let mut env = spec.effective_env();
    env.insert("OPENPILOT_RUST_ROOT".into(), root().into());
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
    let source = required("PANDAD_PADDING_SOURCE_BINARY");
    let binary = required("PANDAD_PADDING_NATIVE_BINARY");
    let python = required("PANDAD_PADDING_PYTHON");
    let source_before = hash(&source);
    let binary_before = hash(&binary);
    let original = pair.join("original");
    let native = pair.join("native");
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
    let expected = invoke(&command(&python, args));
    fs::write(pair.join("original.stdout"), &expected.stdout).unwrap();
    fs::write(pair.join("original.stderr"), &expected.stderr).unwrap();
    let mut native_command = command(&checker(), common(&native));
    let mut native_environment = native_command.effective_env();
    native_environment.insert("PATH".into(), "/nonexistent".into());
    native_command.set_os_env(native_environment);
    let actual = invoke(&native_command);
    fs::write(pair.join("native.stdout"), &actual.stdout).unwrap();
    fs::write(pair.join("native.stderr"), &actual.stderr).unwrap();
    let expected_report = json(original.join("report.json"));
    assert_eq!(expected_report, json(native.join("report.json")));
    assert_eq!(
        expected_report,
        serde_json::from_slice::<Value>(&expected.stdout).unwrap()
    );
    assert_eq!(
        expected_report,
        serde_json::from_slice::<Value>(&actual.stdout).unwrap()
    );
    for label in ["car-params", "source", "native"] {
        // Same actual probes: responses and diagnostics stay byte-identical.
        for suffix in ["stdout", "stderr"] {
            let name = format!("{label}.{suffix}");
            assert_eq!(
                fs::read(original.join(&name)).unwrap(),
                fs::read(native.join(&name)).unwrap(),
                "{name}"
            );
        }
        let name = format!("{label}.input.jsonl");
        let mut expected = rows(&original.join(&name));
        let mut actual = rows(&native.join(&name));
        assert_eq!(expected.len(), if label == "car-params" { 1 } else { 4 });
        assert_eq!(expected.len(), actual.len());
        if label != "car-params" {
            let child = if label == "source" {
                "source-params"
            } else {
                "native-params"
            };
            for (root, rows) in [(&original, &mut expected), (&native, &mut actual)] {
                for row in rows {
                    let params = row.as_object_mut().unwrap().remove("params_root").unwrap();
                    assert_eq!(params.as_str().unwrap(), root.join(child).to_str().unwrap());
                }
            }
        }
        // Only the separately owned Params roots differ. No output fields,
        // wire bytes, allocation fills or request labels are projected away.
        assert_eq!(expected, actual, "{name}");
    }
    let provenance = json(native.join("provenance.json"));
    assert_eq!(provenance["executables"]["source"], source_before);
    assert_eq!(provenance["executables"]["native"], binary_before);
    assert_eq!(hash(&source), source_before);
    assert_eq!(hash(&binary), binary_before);
    // Bind the existing native source builder's complete declared input map,
    // not just the controller's narrower direct-policy admission pins.
    let build = json(required("PANDAD_PADDING_SOURCE_MANIFEST"));
    assert_eq!(build["binary_sha256"], source_before);
    assert_eq!(Path::new(build["binary"].as_str().unwrap()), source);
    let inputs = build["sources"].as_object().unwrap();
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
            .run(&command(&checker(), args.clone()))
            .unwrap();
        let mut original_args = vec![
            "-P".into(),
            root()
                .join("rust/tools/check_pandad_safety_padding.py")
                .into(),
        ];
        original_args.extend(args);
        let original = NativeRunner.run(&command(&python, original_args)).unwrap();
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
            let got = NativeRunner
                .run(&command(
                    if original { &python } else { &native_checker },
                    args,
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
        .env("OPENPILOT_RUST_ROOT", root())
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
