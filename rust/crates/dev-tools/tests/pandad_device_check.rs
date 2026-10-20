//! Contract controls are source transcriptions; actual parity runs separately in CI.
#![cfg(all(target_os = "linux", feature = "pandad-device-native"))]
use openpilot_dev_tools::{
    pandad_device_check as check,
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use std::{
    ffi::OsString,
    fs,
    io::{self, Write},
    os::unix::{ffi::OsStringExt, fs::symlink},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn options(path: &Path) -> check::Options {
    let copied = path.join("source");
    let contract: Value =
        serde_json::from_str(include_str!("fixtures/pandad-device/contract.json")).unwrap();
    for name in contract["sources"].as_object().unwrap().keys() {
        let target = copied.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(root().join(name), target).unwrap();
    }
    let source = path.join("source-probe");
    let native = path.join("native-probe");
    fs::write(&source, b"nonexecuted source identity").unwrap();
    fs::write(&native, b"nonexecuted native identity").unwrap();
    check::Options {
        root: copied,
        source,
        native,
        output: path.join("output"),
        runner: vec![],
    }
}
#[test]
fn dynamic_roster_preserves_all_sources_counts_boundaries_and_rng_order_controls() {
    let cases = check::cases();
    assert_eq!(cases.len(), 1746);
    assert_eq!(
        cases
            .iter()
            .map(|v| v["operations"].as_array().unwrap().len())
            .sum::<usize>(),
        2490
    );
    for (hardware, row) in cases[..256].iter().enumerate() {
        assert_eq!(
            *row,
            json!({"replies":[{"request":193,"count":1,"bytes":[hardware]}],"operations":[]})
        );
    }
    assert_eq!(
        cases[256]["replies"][1],
        json!({"request":210,"count":-4,"bytes":[]})
    );
    assert_eq!(cases[438]["replies"][1]["count"], 58);
    assert_eq!(cases[712]["operations"][0]["op"], "health");
    assert_eq!(cases[712]["operations"][1]["op"], "can_health");
    assert_eq!(cases[1712]["operations"], json!([{"op":"signature"}]));
    assert_eq!(
        cases[1728]["operations"],
        json!([{"op":"serial_read","port":65535}])
    );
    let last = &cases[1745];
    assert_eq!(last["replies"].as_array().unwrap().len(), 5);
    assert_eq!(last["replies"][1]["bytes"][32], 0);
    assert_eq!(last["replies"][1], last["replies"][2]);
    assert_eq!(last["replies"][2], last["replies"][3]);
    assert_eq!(
        last["replies"][4],
        json!({"request":224,"count":0,"bytes":[]})
    );
    // A second generation must restart seed 175, not share a mutable global RNG.
    assert_eq!(check::cases(), cases);
}
#[test]
fn normalization_removes_only_the_original_unspecified_health_float() {
    let mut rows = vec![
        json!({"remaining":0,"results":[null,12,{"health":{"interrupt_load":19.5,"voltage":123},"interrupt_bits":1234},{"other":{"interrupt_load":44}}],"calls":[{"timeout_ms":10}]}),
    ];
    let mut expected = rows.clone();
    expected[0]["results"][2]["health"]
        .as_object_mut()
        .unwrap()
        .remove("interrupt_load");
    check::normalize(&mut rows).unwrap();
    assert_eq!(rows, expected);
    assert!(
        check::normalize(&mut rows).is_err(),
        "missing source field must not be silently ignored"
    );
    assert!(check::normalize(&mut [json!({"results":[{"health":null}]})]).is_err());
}
#[test]
fn equality_keeps_all_fields_order_and_unconsumed_input_checks() {
    let row = json!({"results":[{"bytes":[1,2],"count":1}],"calls":[{"request":10}],"remaining":0});
    assert!(check::compare(std::slice::from_ref(&row), std::slice::from_ref(&row), 1).is_ok());
    let mut numeric = row.clone();
    numeric["results"][0]["count"] = json!(1.0);
    assert!(
        check::compare(std::slice::from_ref(&row), &[numeric], 1).is_ok(),
        "Python JSON numeric equality"
    );
    for part in ["results", "calls", "remaining"] {
        let mut wrong = row.clone();
        wrong.as_object_mut().unwrap().remove(part);
        assert!(check::compare(std::slice::from_ref(&row), &[wrong], 1).is_err());
    }
    let mut leftover = row.clone();
    leftover["remaining"] = json!(1);
    assert!(check::compare(std::slice::from_ref(&leftover), &[leftover.clone()], 1).is_err());
    let mut reordered = row.clone();
    reordered["results"][0]["bytes"] = json!([2, 1]);
    assert!(check::compare(&[row], &[reordered], 1).is_err());
    assert!(check::compare(&[], &[], 1).is_err());
}
#[test]
fn raw_arguments_and_runner_remainder_preserve_original_consumption() {
    let source = OsString::from_vec(b"source-\xff".to_vec());
    let args = vec![
        "--s".into(),
        source.clone(),
        "--n".into(),
        "native".into(),
        "--o".into(),
        "output".into(),
        "--r=/usr/bin/env".into(),
        "--".into(),
        "--help".into(),
    ];
    let parsed = check::parse(&args, &root()).unwrap().unwrap();
    assert_eq!(parsed.source, Path::new(&source));
    assert_eq!(
        parsed.runner,
        vec![OsString::from("/usr/bin/env"), "--".into(), "--help".into()]
    );
    assert!(check::parse(&["--help".into()], &root()).unwrap().is_none());
    for args in [
        vec![],
        vec!["--source"],
        vec!["--source", "s", "--native", "n"],
        vec!["--runner", "--output", "o"],
        vec!["--unknown"],
    ] {
        assert!(check::parse(
            &args.into_iter().map(OsString::from).collect::<Vec<_>>(),
            &root()
        )
        .is_err());
    }
}
struct Replay<'a> {
    calls: Vec<CommandSpec>,
    options: &'a check::Options,
    mutation: &'a str,
    cancel: &'a AtomicBool,
}
impl Runner for Replay<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.calls.push(spec.clone());
        assert_eq!(spec.timeout, Duration::from_secs(60));
        let input = String::from_utf8(spec.input.clone().unwrap()).unwrap();
        let count = input.lines().count();
        assert_eq!(count, 1746);
        if self.calls.len() == 1 && self.mutation == "child-failure" {
            return Ok(Captured {
                code: 23,
                stdout: b"partial child stdout\n".to_vec(),
                stderr: b"source failed\r\n".to_vec(),
            });
        }
        if self.calls.len() == 2 {
            match self.mutation {
                "replace-native" => {
                    fs::rename(
                        &self.options.native,
                        self.options.native.with_extension("old"),
                    )
                    .unwrap();
                    fs::write(&self.options.native, b"nonexecuted native identity").unwrap();
                }
                "source-drift" => {
                    fs::write(
                        self.options.root.join("rust/tools/pandad_device_cases.py"),
                        b"changed during child",
                    )
                    .unwrap();
                }
                "cancel" => self.cancel.store(true, Ordering::Relaxed),
                _ => {}
            }
        }
        let result = json!({"results":[],"calls":[],"remaining":if self.calls.len()==2&&self.mutation=="unconsumed" {1}else{0}});
        Ok(Captured {
            code: 0,
            stdout: format!("{result}\n").repeat(count).into_bytes(),
            stderr: vec![],
        })
    }
}
#[test]
fn controlled_phases_keep_full_payload_native_runner_and_report_shape() {
    let temp = tempfile::tempdir().unwrap();
    let mut options = options(temp.path());
    options.runner = vec!["/usr/bin/env".into(), "--".into()];
    let cancel = AtomicBool::new(false);
    let mut replay = Replay {
        calls: vec![],
        options: &options,
        mutation: "",
        cancel: &cancel,
    };
    let mut stdout = vec![];
    let report = check::check(&options, &mut replay, &mut stdout, &cancel).unwrap();
    assert_eq!(replay.calls.len(), 2);
    assert_eq!(
        replay.calls[0].effective_argv(),
        vec![options.source.clone().into_os_string()]
    );
    assert_eq!(
        replay.calls[1].effective_argv(),
        vec![
            "/usr/bin/env".into(),
            "--".into(),
            options.native.clone().into_os_string()
        ]
    );
    assert_eq!(replay.calls[0].input, replay.calls[1].input);
    assert_eq!(report["scenarios"], 1746);
    assert_eq!(report["operations"], 2490);
    assert_eq!(report, serde_json::from_slice::<Value>(&stdout).unwrap());
    assert_eq!(
        report,
        serde_json::from_slice::<Value>(&fs::read(options.output.join("report.json")).unwrap())
            .unwrap()
    );
}
struct Broken;
impl Write for Broken {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn child_errors_identity_drift_unconsumed_data_and_broken_stdout_never_publish_pass() {
    for mutation in [
        "child-failure",
        "replace-native",
        "source-drift",
        "cancel",
        "unconsumed",
        "broken-stdout",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let options = options(temp.path());
        let cancel = AtomicBool::new(false);
        let mut replay = Replay {
            calls: vec![],
            options: &options,
            mutation,
            cancel: &cancel,
        };
        let mut sink = Vec::new();
        let mut broken = Broken;
        let stdout: &mut dyn Write = if mutation == "broken-stdout" {
            &mut broken
        } else {
            &mut sink
        };
        assert!(
            check::check(&options, &mut replay, stdout, &cancel).is_err(),
            "{mutation}"
        );
        assert!(!options.output.join("report.json").exists());
        if mutation == "child-failure" {
            assert_eq!(
                fs::read(options.output.join("source.stderr")).unwrap(),
                b"source failed\n"
            );
            assert!(!options.output.join("native.jsonl").exists());
        }
    }
}
#[test]
fn exclusive_output_and_changed_dangling_or_fifo_source_pins_reject_before_spawn() {
    struct Never;
    impl Runner for Never {
        fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
            panic!("must not spawn")
        }
    }
    for mutation in ["changed", "dangling", "fifo", "existing-output"] {
        let temp = tempfile::tempdir().unwrap();
        let options = options(temp.path());
        let path = options.root.join("rust/tools/check_pandad_device.py");
        if mutation == "existing-output" {
            fs::create_dir(&options.output).unwrap();
            fs::write(options.output.join("report.json"), b"foreign receipt").unwrap();
        } else {
            fs::remove_file(&path).unwrap();
            match mutation {
                "changed" => fs::write(&path, b"changed source").unwrap(),
                "dangling" => symlink("missing-target", &path).unwrap(),
                _ => rustix::fs::mkfifoat(
                    rustix::fs::CWD,
                    &path,
                    rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
                )
                .unwrap(),
            }
        }
        assert!(check::check(&options, &mut Never, &mut vec![], &AtomicBool::new(false)).is_err());
        if mutation == "existing-output" {
            assert_eq!(
                fs::read(options.output.join("report.json")).unwrap(),
                b"foreign receipt"
            );
        }
    }
}
