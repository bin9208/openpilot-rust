//! Request/phase controls; actual parity runs against independent original capture.
#![cfg(all(target_os = "linux", feature = "pandad-can-io-native"))]
use openpilot_dev_tools::{
    pandad_can_io_check as check,
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use std::{
    ffi::OsString,
    fs,
    io::{self, Write},
    os::unix::ffi::OsStringExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn seeds() -> Vec<Value> {
    include_str!("fixtures/pandad-can-io/seeds.jsonl")
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}
#[test]
fn independently_executed_source_requests_preserve_all_rosters_rng_and_exclusions() {
    let expected = include_str!("fixtures/pandad-can-io/seeds.input.jsonl")
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect::<Vec<Value>>();
    assert_eq!(check::seeds(), expected);
    let packets = check::packets(&seeds()).unwrap();
    let boundaries: Value = serde_json::from_str(include_str!(
        "fixtures/pandad-can-io/request-boundaries-original.json"
    ))
    .unwrap();
    for (maxout, name, count, operations) in
        [(false, "normal", 1085, 1933), (true, "maxout", 1082, 1924)]
    {
        let rows = check::cases(&packets, maxout).unwrap();
        assert_eq!(rows.len(), count);
        assert_eq!(
            rows.iter()
                .map(|r| r["operations"].as_array().unwrap().len())
                .sum::<usize>(),
            operations
        );
        for boundary in boundaries[name].as_array().unwrap() {
            let i = boundary["index"].as_u64().unwrap() as usize;
            assert_eq!(rows[i], boundary["row"], "{name}/{i}");
        }
        if maxout {
            assert!(rows
                .iter()
                .flat_map(|r| r["replies"].as_array().unwrap())
                .all(|reply| reply["healthy"] != true || reply["count"].as_i64().unwrap() >= 0));
        }
    }
    assert!(check::packets(&[]).is_err());
    assert!(check::cases(&[vec![]], false).is_err());
}
#[test]
fn runner_remainder_attached_values_separator_and_help_order_match_argparse() {
    let prefix = [
        "--source", "source", "--native", "native", "--output", "output",
    ]
    .map(OsString::from)
    .to_vec();
    let parse = |tail: Vec<OsString>| {
        let mut args = prefix.clone();
        args.extend(tail);
        check::parse(&args, &root())
    };
    let raw = OsString::from_vec(b"raw-\xff".to_vec());
    let parsed = parse(vec!["--r".into(), "/usr/bin/env".into(), raw.clone()])
        .unwrap()
        .unwrap();
    assert_eq!(parsed.runner, vec![OsString::from("/usr/bin/env"), raw]);
    assert!(parse(vec!["--runner".into(), "r".into(), "--".into()]).is_err());
    assert!(parse(vec![
        "--help".into(),
        "--runner".into(),
        "r".into(),
        "--".into()
    ])
    .unwrap()
    .is_none());
    assert!(parse(vec!["--runner=r".into(), "--help".into()])
        .unwrap()
        .is_none());
    assert_eq!(
        parse(vec!["--runner=--".into()]).unwrap().unwrap().runner,
        vec![OsString::from("--")]
    );
    assert!(parse(vec!["--runner=r".into(), "tail".into()]).is_err());
    assert!(parse(vec!["--runner=r".into(), "--".into(), "--help".into()]).is_err());
    assert_eq!(
        parse(vec!["--runner=first".into(), "--runner=last".into()])
            .unwrap()
            .unwrap()
            .runner,
        vec![OsString::from("last")]
    );
    assert!(check::parse(&[], &root()).is_err());
}
fn options(directory: &Path) -> check::Options {
    let copied = directory.join("root");
    for name in [
        "rust/tools/check_pandad_can_io.py",
        "rust/tools/pandad_protocol_cases.py",
        "rust/tools/pandad_can_io_source.cc",
        "openpilot/selfdrive/pandad/panda.cc",
        "openpilot/selfdrive/pandad/panda.h",
    ] {
        let source = root().join(name);
        if !source.exists() && name.ends_with(".py") {
            continue;
        }
        let target = copied.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(source, target).unwrap();
    }
    let source = directory.join("source");
    let native = directory.join("native");
    fs::write(&source, b"source identity").unwrap();
    fs::write(&native, b"native identity").unwrap();
    check::Options {
        root: copied,
        source,
        native,
        output: directory.join("output"),
        runner: vec![],
    }
}
struct Replay<'a> {
    calls: usize,
    mutation: &'a str,
    options: &'a check::Options,
    flag: &'a AtomicBool,
}
impl Runner for Replay<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        assert_eq!(spec.timeout, Duration::from_secs(90));
        assert!(!spec.disable_core_dump);
        if self.calls <= 3 {
            assert!(spec.env_remove.contains(&"PANDAD_MAXOUT".into()));
        } else {
            assert_eq!(
                spec.os_env
                    .as_ref()
                    .unwrap()
                    .get(&OsString::from("PANDAD_MAXOUT")),
                Some(&OsString::from("1"))
            );
            assert!(!spec.env_remove.contains(&"PANDAD_MAXOUT".into()));
        }
        let rows = String::from_utf8(spec.input.clone().unwrap())
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .collect::<Vec<_>>();
        let mut values = if self.calls == 1 {
            seeds()
        } else {
            vec![json!({"results":[],"unused_replies":0}); rows.len()]
        };
        if self.calls == 3 {
            match self.mutation {
                "mismatch" => values[0]["different"] = json!(true),
                "unused" => values[0]["unused_replies"] = json!(1),
                "cancel" => self.flag.store(true, Ordering::Relaxed),
                "source-drift" => fs::write(
                    self.options
                        .root
                        .join("rust/tools/pandad_protocol_cases.py"),
                    b"changed",
                )
                .unwrap(),
                _ => {}
            }
        }
        if self.mutation == "child-failure" && self.calls == 2 {
            return Ok(Captured {
                code: 17,
                stdout: b"partial\n".to_vec(),
                stderr: b"source error\n".to_vec(),
            });
        }
        Ok(Captured {
            code: 0,
            stdout: values
                .iter()
                .map(|row| format!("{row}\n"))
                .collect::<String>()
                .into_bytes(),
            stderr: vec![],
        })
    }
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
fn all_five_phases_enforce_environment_source_order_reply_consumption_and_publication() {
    for mutation in [
        "",
        "child-failure",
        "mismatch",
        "unused",
        "cancel",
        "source-drift",
        "broken-stdout",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let options = options(directory.path());
        let flag = AtomicBool::new(false);
        let mut replay = Replay {
            calls: 0,
            mutation,
            options: &options,
            flag: &flag,
        };
        let mut bytes = Vec::new();
        let mut broken = Broken;
        let stdout: &mut dyn Write = if mutation == "broken-stdout" {
            &mut broken
        } else {
            &mut bytes
        };
        let result = check::check(&options, &mut replay, stdout, &flag);
        if mutation.is_empty() {
            let report: Value =
                serde_json::from_str(&serde_json::to_string(&result.unwrap()).unwrap()).unwrap();
            assert_eq!(replay.calls, 5);
            assert_eq!(
                report["runs"],
                json!([{"maxout":false,"scenarios":1085,"operations":1933},{"maxout":true,"scenarios":1082,"operations":1924}])
            );
        } else {
            assert!(result.is_err(), "{mutation}");
            assert!(!options.output.join("report.json").exists());
            assert!(!options.output.join("provenance.json").exists());
            if mutation == "child-failure" {
                assert_eq!(replay.calls, 2);
                assert!(!options.output.join("normal/native.input.jsonl").exists());
            }
            if mutation == "mismatch" {
                assert!(options.output.join("normal/mismatch.json").exists());
            }
        }
    }
}
#[test]
fn raw_native_and_runner_paths_survive_complete_report_and_both_retired_inputs_may_be_absent() {
    use openpilot_log_format::{JsonValue, JsonView};
    let directory = tempfile::tempdir().unwrap();
    let mut options = options(directory.path());
    for name in [
        "rust/tools/check_pandad_can_io.py",
        "rust/tools/pandad_protocol_cases.py",
    ] {
        match fs::remove_file(options.root.join(name)) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => panic!("{error}"),
        }
    }
    let native = directory
        .path()
        .join(OsString::from_vec(b"native-\xff".to_vec()));
    fs::rename(&options.native, &native).unwrap();
    options.native = native;
    let runner = directory
        .path()
        .join(OsString::from_vec(b"runner-\xfe".to_vec()));
    fs::write(&runner, b"mocked held runner").unwrap();
    options.runner = vec![runner.clone().into_os_string()];
    let flag = AtomicBool::new(false);
    let mut replay = Replay {
        calls: 0,
        mutation: "",
        options: &options,
        flag: &flag,
    };
    let mut stdout = Vec::new();
    let report = check::check(&options, &mut replay, &mut stdout, &flag).unwrap();
    let report = report.json_value().unwrap();
    let JsonView::Array(command) = report.get("native_command").unwrap().view() else {
        panic!("full command missing")
    };
    assert_eq!(
        command
            .iter()
            .map(|v| openpilot_dev_tools::python_fs::from_json(v).unwrap())
            .collect::<Vec<_>>(),
        vec![runner.into_os_string(), options.native.into_os_string()]
    );
    assert_eq!(
        report.to_json().unwrap(),
        JsonValue::parse(std::str::from_utf8(&stdout).unwrap())
            .unwrap()
            .to_json()
            .unwrap()
    );
}

#[test]
fn absent_shared_corpus_rejects_a_remaining_original_protocol_caller_before_children() {
    let directory = tempfile::tempdir().unwrap();
    let options = options(directory.path());
    for name in [
        "rust/tools/check_pandad_can_io.py",
        "rust/tools/pandad_protocol_cases.py",
    ] {
        let path = options.root.join(name);
        if path.exists() {
            fs::remove_file(path).unwrap();
        }
    }
    fs::write(
        options.root.join("rust/tools/check_pandad_protocol.py"),
        b"remaining original caller",
    )
    .unwrap();
    let flag = AtomicBool::new(false);
    let mut replay = Replay {
        calls: 0,
        mutation: "",
        options: &options,
        flag: &flag,
    };
    let mut stdout = Vec::new();
    let error = check::check(&options, &mut replay, &mut stdout, &flag)
        .err()
        .unwrap();
    assert!(error.to_string().contains("still requires shared corpus"));
    assert_eq!(replay.calls, 0);
    assert!(stdout.is_empty());
    assert!(!options.output.join("report.json").exists());
}
