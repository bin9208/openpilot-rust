//! Source-derived contracts and test doubles; real parity is a separate mandatory CI pair.
#![cfg(all(target_os = "linux", feature = "pandad-safety-padding-native"))]
use openpilot_dev_tools::{
    pandad_safety_padding_check::{self as check, Command, Options},
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn contract() -> Value {
    serde_json::from_slice(include_bytes!(
        "fixtures/pandad-safety-padding/contract.json"
    ))
    .unwrap()
}
fn captured_original() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pandad-safety-padding/edb42bf0-original")
}
fn historical_present() -> bool {
    let path = root().join("rust/tools/check_pandad_safety_padding.py");
    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            assert!(metadata.file_type().is_file());
            assert_eq!(
                format!("{:x}", Sha256::digest(fs::read(path).unwrap())),
                "f0d0867a9481a698df8da04d472bdf2e2d6aa77da2a3ae29fba08aae74d5a836"
            );
            true
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => panic!("historical checker admission: {error}"),
    }
}
fn captured_rows(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
#[test]
fn sealed_executed_original_fixture_retains_actual_wire_rows_and_report() {
    const MANIFEST_SHA: &str = "3a12023217195181b9f1d05d06803a4b0add0438baf700abfeb5a7ad0f3f22ea";
    let folder = captured_original();
    let manifest = fs::read(folder.join("SHA256SUMS")).unwrap();
    assert_eq!(format!("{:x}", Sha256::digest(&manifest)), MANIFEST_SHA);
    let listed = std::str::from_utf8(&manifest)
        .unwrap()
        .lines()
        .map(|line| {
            let (digest, name) = line.split_once("  ").unwrap();
            assert!(!name.contains('/') && !name.contains(".."));
            assert_eq!(
                format!("{:x}", Sha256::digest(fs::read(folder.join(name)).unwrap())),
                digest,
                "{name}"
            );
            name
        })
        .collect::<Vec<_>>();
    assert_eq!(listed.len(), 12);
    assert_eq!(
        captured_rows(&folder.join("car-params.input.jsonl")),
        vec![contract()["encoding_request"].clone()]
    );
    let encoded = captured_rows(&folder.join("car-params.stdout"));
    let wire = encoded[0]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| u8::try_from(n.as_u64().unwrap()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(wire.len(), 312);
    let source = captured_rows(&folder.join("source.stdout"));
    let native = captured_rows(&folder.join("native.stdout"));
    let observed = check::compare(wire.len(), &source, &native).unwrap();
    let report: Value =
        serde_json::from_slice(&fs::read(folder.join("report.json")).unwrap()).unwrap();
    assert_eq!(observed, report);
    assert_eq!(
        report,
        serde_json::from_slice::<Value>(&fs::read(folder.join("original.stdout")).unwrap())
            .unwrap()
    );
    assert_eq!(report["source_truncated_final_commands"][0]["index"], 4369);
    assert_eq!(report["source_truncated_final_commands"][1]["index"], 8738);
    assert!(fs::read(folder.join("original.stderr")).unwrap().is_empty());
    let expected = check::cases(&wire).unwrap();
    for (name, child) in [("source", "source-params"), ("native", "native-params")] {
        let rows = captured_rows(&folder.join(format!("{name}.input.jsonl")));
        assert_eq!(rows.len(), 4);
        for (index, mut row) in rows.into_iter().enumerate() {
            let params = row.as_object_mut().unwrap().remove("params_root").unwrap();
            assert_eq!(params.as_str().unwrap(), format!("/home/runner/work/_temp/panda-native/padding-controller/pair-vsc2ns/original/{child}"));
            assert_eq!(row, expected[index]);
        }
    }
}
fn responses() -> (Vec<Value>, Vec<Value>) {
    let valid = json!({"failed":false,"changed":[],"logs":[{"level":30,"message":"source fixture control"}],
        "commands":[{"panda":1,"request":220,"value":17,"index":65435}],
        "state":{"initialized":true,"safety_configured":true,"log_once":true,"prev_obd_multiplexing":false}});
    let mut source = Vec::new();
    let mut native = Vec::new();
    for index in 0..4 {
        let mut original = valid.clone();
        original["filled_allocations"] = json!(1);
        if index >= 2 {
            original["commands"][0]["index"] = json!(if index == 2 { 4369 } else { 8738 });
        }
        source.push(json!({"results":[original]}));
        let mut got = valid.clone();
        if index >= 2 {
            got["failed"] = json!(true);
            got["state"]["safety_configured"] = json!(false);
            got["commands"] = json!([
                {"panda":0,"request":220,"value":3,"index":1},
                {"panda":1,"request":220,"value":3,"index":1}]);
        }
        native.push(json!({"results":[got]}));
    }
    (source, native)
}
#[test]
fn original_source_pins_remain_present_and_unchanged() {
    for (name, expected) in contract()["source_sha256"].as_object().unwrap() {
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(root().join(name)).unwrap())),
            expected.as_str().unwrap(),
            "{name}"
        );
    }
}
#[test]
fn complete_four_case_roster_retains_order_fill_values_and_exact_last_word_truncation() {
    let wire = (0..24).collect::<Vec<u8>>();
    let rows = check::cases(&wire).unwrap();
    let captured = contract();
    assert_eq!(rows.len(), 4);
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row["label"], captured["labels"][index]);
        assert_eq!(row["pandas"], 2);
        assert_eq!(row["operations"].as_array().unwrap().len(), 1);
        let operation = &row["operations"][0];
        assert_eq!(operation["onroad"], true);
        assert_eq!(operation["allocation_fill"], captured["fill_bytes"][index]);
        assert_eq!(operation["params"]["FirmwareQueryDone"], json!([49]));
        assert_eq!(operation["params"]["ControlsReady"], json!([49]));
        assert_eq!(
            operation["params"]["CarParams"],
            json!(&wire[..if index < 2 { 24 } else { 16 }])
        );
    }
    assert!(check::cases(&[0; 7]).is_err());
}
#[test]
fn comparison_keeps_both_valid_controls_and_deliberate_truncated_difference() {
    let (source, native) = responses();
    let got = check::compare(24, &source, &native).unwrap();
    assert_eq!(got["full_message_bytes"], 24);
    assert_eq!(got["truncated_message_bytes"], 16);
    assert_eq!(
        got["source_truncated_final_commands"],
        json!([
        {"panda":1,"request":220,"value":17,"index":4369},
        {"panda":1,"request":220,"value":17,"index":8738}])
    );
    assert_eq!(got["native_rejected_both_truncated_inputs"], true);
}
#[test]
fn independent_corruption_controls_reject_every_original_assertion_boundary() {
    for boundary in 0..10 {
        let (mut source, mut native) = responses();
        match boundary {
            0 => {
                source.pop();
            }
            1 => source[0]["results"][0]["filled_allocations"] = json!(0),
            2 => native[1]["results"][0]["logs"] = json!([]),
            3 => source[1]["results"][0]["commands"][0]["index"] = json!(42),
            4 => source[2]["results"][0]["filled_allocations"] = json!(0),
            5 => source[2]["results"][0]["failed"] = json!(true),
            6 => source[3]["results"][0]["commands"][0]["index"] = json!(4369),
            7 => native[2]["results"][0]["failed"] = json!(false),
            8 => native[3]["results"][0]["state"]["safety_configured"] = json!(true),
            _ => native[3]["results"][0]["commands"][0]["index"] = json!(0),
        }
        assert!(
            check::compare(24, &source, &native).is_err(),
            "boundary {boundary}"
        );
    }
}
fn copied_source(path: &Path) {
    for name in contract()["source_sha256"].as_object().unwrap().keys() {
        let destination = path.join(name);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::copy(root().join(name), destination).unwrap();
    }
}
struct Replay {
    calls: Vec<CommandSpec>,
    fail: bool,
}
impl Runner for Replay {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.calls.push(spec.clone());
        let index = self.calls.len() - 1;
        let (source, native) = responses();
        let rows = match index {
            0 => vec![json!([
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15
            ])],
            1 => source,
            _ => native,
        };
        Ok(Captured {
            code: if self.fail { 7 } else { 0 },
            stdout: rows
                .iter()
                .map(|row| format!("{row}\n"))
                .collect::<String>()
                .into_bytes(),
            stderr: b"owned fixture stderr\r\n".to_vec(),
        })
    }
}
fn options(temp: &Path) -> Options {
    let copied = temp.join("source-root");
    copied_source(&copied);
    let source = temp.join("source-probe");
    fs::write(&source, b"test double input, not executable").unwrap();
    let binary = temp.join("native-probe");
    fs::write(&binary, b"test double input, not executable").unwrap();
    Options {
        root: copied,
        source,
        binary,
        output: temp.join("output"),
    }
}
#[test]
fn three_subprocess_phases_keep_input_artifacts_counts_params_roots_and_ninety_second_bounds() {
    let temp = tempfile::tempdir().unwrap();
    let options = options(temp.path());
    let mut replay = Replay {
        calls: vec![],
        fail: false,
    };
    let mut stdout = Vec::new();
    let completed =
        check::execute(&options, &mut replay, &AtomicBool::new(false), &mut stdout).unwrap();
    assert_eq!(replay.calls.len(), 3);
    for (index, call) in replay.calls.iter().enumerate() {
        assert_eq!(call.timeout, Duration::from_secs(90));
        assert_eq!(
            call.effective_argv(),
            vec![if index < 2 {
                options.source.clone()
            } else {
                options.binary.clone()
            }
            .into_os_string()]
        );
        let rows = std::str::from_utf8(call.input.as_ref().unwrap())
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        if index == 0 {
            assert_eq!(rows, vec![contract()["encoding_request"].clone()]);
        } else {
            assert_eq!(rows.len(), 4);
            for row in rows {
                assert_eq!(
                    Path::new(row["params_root"].as_str().unwrap()),
                    options.output.join(if index == 1 {
                        "source-params"
                    } else {
                        "native-params"
                    })
                );
            }
        }
    }
    assert_eq!(
        serde_json::from_slice::<Value>(&stdout).unwrap(),
        completed.report
    );
    assert_eq!(
        fs::read_to_string(options.output.join("source.stderr")).unwrap(),
        "owned fixture stderr\n"
    );
    let saved: Value =
        serde_json::from_slice(&fs::read(options.output.join("report.json")).unwrap()).unwrap();
    assert_eq!(saved, completed.report);
}
#[test]
fn child_failure_and_broken_stdout_never_publish_a_report() {
    for child_failure in [true, false] {
        let temp = tempfile::tempdir().unwrap();
        let options = options(temp.path());
        let mut replay = Replay {
            calls: vec![],
            fail: child_failure,
        };
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        assert!(
            check::execute(&options, &mut replay, &AtomicBool::new(false), &mut Broken).is_err()
        );
        assert!(!options.output.join("report.json").exists());
        assert_eq!(replay.calls.len(), if child_failure { 1 } else { 3 });
        assert!(options.output.join("car-params.stdout").exists());
    }
}
#[test]
fn surviving_source_drift_and_existing_output_are_rejected_without_probe_execution() {
    let temp = tempfile::tempdir().unwrap();
    let options = options(temp.path());
    fs::write(
        options.root.join("rust/tools/check_pandad_safety.py"),
        b"changed original",
    )
    .unwrap();
    let mut replay = Replay {
        calls: vec![],
        fail: false,
    };
    assert!(check::execute(
        &options,
        &mut replay,
        &AtomicBool::new(false),
        &mut Vec::new()
    )
    .is_err());
    assert!(replay.calls.is_empty());
    fs::write(options.output.join("report.json"), b"foreign receipt").unwrap();
    assert!(check::execute(
        &options,
        &mut replay,
        &AtomicBool::new(false),
        &mut Vec::new()
    )
    .is_err());
    assert_eq!(
        fs::read(options.output.join("report.json")).unwrap(),
        b"foreign receipt"
    );
}
#[test]
fn cli_rejects_missing_values_unknown_flags_and_preserves_raw_os_paths() {
    use std::os::unix::ffi::OsStringExt;
    assert!(matches!(
        check::parse(&["--help".into()], &root()).unwrap(),
        Command::Help
    ));
    for args in [
        vec![],
        vec!["--source"],
        vec!["--unknown"],
        vec!["--source", "s", "--binary", "b"],
    ] {
        assert!(check::parse(
            &args.into_iter().map(Into::into).collect::<Vec<_>>(),
            &root()
        )
        .is_err());
    }
    let raw = OsString::from_vec(b"owned-\xff-probe".to_vec());
    let Command::Run(options) = check::parse(
        &[
            "--source".into(),
            raw.clone(),
            "--binary".into(),
            "b".into(),
            "--output".into(),
            "o".into(),
        ],
        &root(),
    )
    .unwrap() else {
        panic!("run");
    };
    assert_eq!(options.source.file_name(), Some(raw.as_os_str()));
}
#[test]
fn stdout_boundary_mutation_rejects_changed_private_input_before_receipt_publication() {
    let temp = tempfile::tempdir().unwrap();
    let options = options(temp.path());
    struct Mutating {
        path: PathBuf,
    }
    impl Write for Mutating {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            fs::write(&self.path, b"changed private source during stdout")?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut replay = Replay {
        calls: vec![],
        fail: false,
    };
    let mut stdout = Mutating {
        path: options.root.join("rust/tools/check_pandad_safety.py"),
    };
    assert!(check::execute(&options, &mut replay, &AtomicBool::new(false), &mut stdout).is_err());
    assert_eq!(replay.calls.len(), 3);
    assert!(!options.output.join("report.json").exists());
}
#[test]
fn exact_historical_checker_reappearance_and_missing_parent_are_rejected_before_launch() {
    for kind in [
        "changed-file",
        "symlink",
        "dangling-symlink",
        "fifo",
        "directory",
        "missing-parent",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let options = options(temp.path());
        let historical = options
            .root
            .join("rust/tools/check_pandad_safety_padding.py");
        match kind {
            "changed-file" => fs::write(&historical, b"historical checker returned").unwrap(),
            "symlink" => std::os::unix::fs::symlink("check_pandad_safety.py", &historical).unwrap(),
            "dangling-symlink" => {
                std::os::unix::fs::symlink("missing-target", &historical).unwrap()
            }
            "fifo" => rustix::fs::mkfifoat(
                rustix::fs::CWD,
                &historical,
                rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
            )
            .unwrap(),
            "directory" => fs::create_dir(&historical).unwrap(),
            _ => fs::remove_dir_all(options.root.join("rust/tools")).unwrap(),
        }
        let mut replay = Replay {
            calls: vec![],
            fail: false,
        };
        assert!(
            check::execute(
                &options,
                &mut replay,
                &AtomicBool::new(false),
                &mut Vec::new()
            )
            .is_err(),
            "{kind}"
        );
        assert!(replay.calls.is_empty(), "{kind}");
        assert!(!options.output.join("report.json").exists(), "{kind}");
    }
}
#[test]
fn exact_original_presence_is_admitted_but_presence_changes_fail_after_stdout() {
    let mut states = vec![(false, "reappear")];
    if historical_present() {
        states.extend([(true, "remove"), (true, "mutate")]);
    } else {
        // A missing historical source remains a tested admission state. The
        // independently sealed original observation stays mandatory above.
        assert_eq!(states.len(), 1);
    }
    for (initially_present, change) in states {
        let temp = tempfile::tempdir().unwrap();
        let options = options(temp.path());
        let historical = options
            .root
            .join("rust/tools/check_pandad_safety_padding.py");
        if initially_present {
            fs::copy(
                root().join("rust/tools/check_pandad_safety_padding.py"),
                &historical,
            )
            .unwrap();
        }
        struct ChangePresence {
            path: PathBuf,
            change: &'static str,
        }
        impl Write for ChangePresence {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                match self.change {
                    "remove" => fs::remove_file(&self.path)?,
                    _ => fs::write(&self.path, b"changed or reappeared")?,
                }
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut replay = Replay {
            calls: vec![],
            fail: false,
        };
        assert!(check::execute(
            &options,
            &mut replay,
            &AtomicBool::new(false),
            &mut ChangePresence {
                path: historical,
                change
            }
        )
        .is_err());
        assert_eq!(replay.calls.len(), 3);
        assert!(!options.output.join("report.json").exists());
    }
}
#[test]
fn late_cancellation_removes_only_a_descriptor_reserved_owned_report() {
    use std::os::unix::fs::MetadataExt;
    let temp = tempfile::tempdir().unwrap();
    let options = options(temp.path());
    let mut replay = Replay {
        calls: vec![],
        fail: false,
    };
    let completed = check::execute(
        &options,
        &mut replay,
        &AtomicBool::new(false),
        &mut Vec::new(),
    )
    .unwrap();
    let path = options.output.join("report.json");
    let original = fs::metadata(&path).unwrap();
    fs::remove_file(&path).unwrap();
    fs::write(&path, b"foreign receipt").unwrap();
    let replacement = fs::metadata(&path).unwrap();
    assert_ne!(
        (original.dev(), original.ino()),
        (replacement.dev(), replacement.ino())
    );
    assert!(completed.finish(&AtomicBool::new(true)).is_err());
    assert_eq!(fs::read(path).unwrap(), b"foreign receipt");
}
