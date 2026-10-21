//! Controller controls and independently executed original request boundaries.
#![cfg(all(target_os = "linux", feature = "pandad-protocol-native"))]
use openpilot_dev_tools::{
    pandad_protocol_check as check,
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
fn original_seeds() -> Vec<Value> {
    include_str!("fixtures/pandad-protocol/seeds-original.jsonl")
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}
#[test]
fn original_request_fields_rng_order_and_wrapping_boundaries_are_preserved() {
    let expected = include_str!("fixtures/pandad-protocol/seed-requests-original.jsonl")
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect::<Vec<Value>>();
    assert_eq!(check::seeds(), expected);
    let mut rows = check::packing();
    assert_eq!(rows.len(), 664);
    let decoding = check::decoding(&original_seeds()).unwrap();
    assert_eq!(decoding.len(), 2380);
    rows.extend(decoding);
    let alerts = check::alerts();
    assert_eq!(alerts.len(), 130);
    rows.extend(alerts);
    assert_eq!(rows.len(), 3174);
    assert_eq!(
        rows.iter()
            .map(|r| r["operations"].as_array().map_or(0, Vec::len))
            .sum::<usize>(),
        30720
    );
    let boundaries: Value = serde_json::from_str(include_str!(
        "fixtures/pandad-protocol/request-boundaries-original.json"
    ))
    .unwrap();
    for boundary in boundaries.as_array().unwrap() {
        let i = boundary["index"].as_u64().unwrap() as usize;
        assert_eq!(rows[i], boundary["row"], "original case {i}");
    }
    assert_eq!(check::packing(), rows[..664]);
}
#[test]
fn invalid_seed_flags_and_missing_alert_time_never_silently_pass() {
    assert!(check::decoding(&[]).is_err());
    let mut seed = original_seeds();
    seed[0]["chunks"] = json!([[]]);
    assert!(check::decoding(&seed).is_err());
    let flag = json!({"results":[{"ok":true,"resets":0,"frames":[{}]}]});
    assert!(check::validate_flags(std::slice::from_ref(&flag), 0, 1).is_ok());
    for field in ["ok", "resets", "frames"] {
        let mut bad = flag.clone();
        bad["results"][0].as_object_mut().unwrap().remove(field);
        assert!(check::validate_flags(&[bad], 0, 1).is_err());
    }
    let request = json!({"op":"alerts"});
    let mut value = json!({"results":[{"since":"18446744073709551615","pending_since":"0","times":["9"],"other":"kept"}]});
    check::normalize(
        std::slice::from_ref(&request),
        std::slice::from_mut(&mut value),
    )
    .unwrap();
    assert_eq!(value["results"][0]["since"], u64::MAX);
    assert_eq!(value["results"][0]["other"], "kept");
    assert!(check::normalize(
        &[request],
        &mut [json!({"results":[{"since":"0","times":[]}]})]
    )
    .is_err());
}
#[test]
fn cli_help_required_pair_and_raw_option_consumption() {
    assert!(check::parse(&["--help".into()], &root()).unwrap().is_none());
    assert!(check::parse(&[], &root()).is_err());
    let args = [
        "--source", "s", "--binary", "b", "--output", "o", "--qemu", "q",
    ]
    .map(Into::into);
    assert!(check::parse(&args, &root()).is_err());
    let args = [
        "--source",
        "s",
        "--binary",
        "b",
        "--output",
        "o",
        "--qemu",
        "q",
        "--sysroot",
        "r",
    ]
    .map(Into::into);
    let options = check::parse(&args, &root()).unwrap().unwrap();
    assert_eq!(options.qemu, Some("q".into()));
}
fn options(directory: &Path) -> check::Options {
    let copied = directory.join("root");
    for name in [
        "rust/tools/check_pandad_protocol.py",
        "rust/tools/pandad_protocol_cases.py",
        "rust/tools/pandad_protocol_source.cc",
        "openpilot/selfdrive/pandad/panda.cc",
        "openpilot/selfdrive/pandad/panda.h",
        "openpilot/selfdrive/pandad/spi_alert.h",
    ] {
        let source = root().join(name);
        if name.ends_with("check_pandad_protocol.py") && !source.exists() {
            continue;
        }
        let target = copied.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(source, target).unwrap();
    }
    let source = directory.join("source");
    let binary = directory.join("native");
    fs::write(&source, b"source identity").unwrap();
    fs::write(&binary, b"native identity").unwrap();
    check::Options {
        root: copied,
        source,
        binary,
        output: directory.join("output"),
        qemu: None,
        sysroot: None,
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
        assert!(spec.disable_core_dump);
        let requests = String::from_utf8(spec.input.clone().unwrap())
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .collect::<Vec<_>>();
        if spec.timeout == Duration::from_secs(3) {
            return Ok(Captured {
                code: -6,
                stdout: vec![],
                stderr: b"invalid length\n".to_vec(),
            });
        }
        assert_eq!(spec.timeout, Duration::from_secs(45));
        let mut values = if self.calls == 1 {
            original_seeds()
        } else if requests.len() == 1 {
            vec![json!({"chunks":[]})]
        } else {
            requests
                .iter()
                .map(|_| json!({"results":[{"ok":true,"resets":0,"frames":[{}]}]}))
                .collect::<Vec<_>>()
        };
        // Alerts require their exact integer-normalization fields, independently of codec policy.
        for (request, value) in requests.iter().zip(&mut values) {
            if request["op"] == "alerts" {
                *value = json!({"results":[{"since":"0","pending_since":"0","times":[]}]});
            }
        }
        if self.calls == 3 {
            match self.mutation {
                "mismatch" => values[0] = json!({"different":true}),
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
fn controller_runs_all_71_phases_and_rejects_mismatch_drift_cancel_or_broken_publication() {
    for mutation in ["", "mismatch", "source-drift", "cancel", "broken-stdout"] {
        let directory = tempfile::tempdir().unwrap();
        let options = options(directory.path());
        let flag = AtomicBool::new(false);
        let mut replay = Replay {
            calls: 0,
            mutation,
            options: &options,
            flag: &flag,
        };
        let mut sink = Vec::new();
        let mut broken = Broken;
        let stdout: &mut dyn Write = if mutation == "broken-stdout" {
            &mut broken
        } else {
            &mut sink
        };
        let result = check::check(&options, &mut replay, stdout, &flag);
        if mutation.is_empty() {
            let report: Value =
                serde_json::from_str(&serde_json::to_string(&result.unwrap()).unwrap()).unwrap();
            assert_eq!(replay.calls, 71);
            assert_eq!(report["cases"], 3174);
            assert_eq!(report["invalid_lengths"].as_array().unwrap().len(), 17);
        } else {
            assert!(result.is_err(), "{mutation}");
            assert!(!options.output.join("report.json").exists());
            assert!(!options.output.join("provenance.json").exists());
        }
    }
}

#[test]
fn raw_native_qemu_sysroot_paths_survive_report_return_stdout_and_receipt() {
    use openpilot_log_format::{JsonValue, JsonView};
    let directory = tempfile::tempdir().unwrap();
    let mut options = options(directory.path());
    let native = directory
        .path()
        .join(OsString::from_vec(b"native-\xff".to_vec()));
    fs::rename(&options.binary, &native).unwrap();
    options.binary = native;
    let qemu = directory
        .path()
        .join(OsString::from_vec(b"qemu-\xfe".to_vec()));
    fs::write(&qemu, b"held mocked runner").unwrap();
    let sysroot = directory
        .path()
        .join(OsString::from_vec(b"sysroot-\xfd".to_vec()));
    fs::create_dir(&sysroot).unwrap();
    options.qemu = Some(qemu.clone());
    options.sysroot = Some(sysroot.clone());
    let flag = AtomicBool::new(false);
    let mut replay = Replay {
        calls: 0,
        mutation: "",
        options: &options,
        flag: &flag,
    };
    let mut stdout = Vec::new();
    let report = check::check(&options, &mut replay, &mut stdout, &flag).unwrap();
    assert_eq!(replay.calls, 71);
    let json = report.json_value().unwrap();
    let JsonView::Array(command) = json.get("native_command").unwrap().view() else {
        panic!("complete command array missing")
    };
    let decoded = command
        .iter()
        .map(|value| openpilot_dev_tools::python_fs::from_json(value).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        decoded,
        vec![
            qemu.into_os_string(),
            "-L".into(),
            sysroot.into_os_string(),
            options.binary.clone().into_os_string()
        ]
    );
    let printed = JsonValue::parse(std::str::from_utf8(&stdout).unwrap()).unwrap();
    let persisted =
        JsonValue::parse(&fs::read_to_string(options.output.join("report.json")).unwrap()).unwrap();
    assert_eq!(json.to_json().unwrap(), printed.to_json().unwrap());
    assert_eq!(json.to_json().unwrap(), persisted.to_json().unwrap());
}
