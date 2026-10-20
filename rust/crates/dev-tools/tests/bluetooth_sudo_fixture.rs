#![cfg(target_os = "linux")]
use openpilot_dev_tools::{
    python_fs,
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde_json::json;
use std::{
    ffi::{OsStr, OsString},
    fs,
    os::unix::ffi::OsStringExt,
    path::Path,
    time::{Duration, Instant},
};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_bluetooth-sudo-fixture")
}
fn call(log: &Path, mode: Option<&str>, args: &[OsString]) -> Captured {
    pair_child(None, log, mode.map(OsStr::new), args).unwrap().0
}
fn pair_child(
    source: Option<&Path>,
    log: &Path,
    mode: Option<&OsStr>,
    args: &[OsString],
) -> Result<(Captured, Duration)> {
    let mut argv = if let Some(source) = source {
        vec![OsString::from("python"), source.as_os_str().to_os_string()]
    } else {
        vec![OsString::from(binary())]
    };
    argv.extend_from_slice(args);
    let mut spec = CommandSpec::new_os(argv, Duration::from_secs(20));
    let mut environment = std::collections::BTreeMap::new();
    environment.insert(
        OsString::from("INPUT_FIXTURE_SUDO_LOG"),
        log.as_os_str().to_os_string(),
    );
    if let Some(mode) = mode {
        environment.insert(
            OsString::from("INPUT_FIXTURE_SUDO_MODE"),
            mode.to_os_string(),
        );
    } else {
        spec.env_remove.push("INPUT_FIXTURE_SUDO_MODE".into());
    }
    spec.set_os_env(environment);
    spec.stdout_limit = 2 * 1024 * 1024;
    spec.stderr_limit = 2 * 1024 * 1024;
    let start = Instant::now();
    let output = NativeRunner.run(&spec)?;
    Ok((output, start.elapsed()))
}
fn chgrp(path: OsString) -> Vec<OsString> {
    vec!["-n".into(), "chgrp".into(), "gpio".into(), path]
}
fn chmod(path: OsString) -> Vec<OsString> {
    vec!["-n".into(), "chmod".into(), "g+rw".into(), path]
}
#[test]
fn actual_child_appends_python_escaped_argv_and_selects_both_failures() {
    let temp = tempfile::tempdir().unwrap();
    let log = temp.path().join("sudo.jsonl");
    let raw = OsString::from_vec(b"/dev/input/event\xff".to_vec());
    let first = call(&log, None, &chgrp(raw));
    assert_eq!(first.code, 0);
    assert!(first.stdout.is_empty() && first.stderr.is_empty());
    let second = call(&log, Some("fail-chgrp"), &chgrp("/dev/input/event1".into()));
    assert_eq!(second.code, 7);
    let third = call(&log, Some("fail-chgrp"), &chmod("/dev/input/event1".into()));
    assert_eq!(third.code, 0);
    let fourth = call(&log, Some("fail-chmod"), &chmod("/dev/input/event1".into()));
    assert_eq!(fourth.code, 7);
    assert_eq!(
        fs::read_to_string(log).unwrap(),
        "[\"-n\", \"chgrp\", \"gpio\", \"/dev/input/event\\udcff\"]\n\
         [\"-n\", \"chgrp\", \"gpio\", \"/dev/input/event1\"]\n\
         [\"-n\", \"chmod\", \"g+rw\", \"/dev/input/event1\"]\n\
         [\"-n\", \"chmod\", \"g+rw\", \"/dev/input/event1\"]\n"
    );
}
#[test]
fn actual_child_loud_signal_and_ten_second_timeout_modes() {
    let temp = tempfile::tempdir().unwrap();
    let log = temp.path().join("sudo.jsonl");
    let args = chgrp("/dev/input/event1".into());
    let loud = call(&log, Some("loud"), &args);
    assert_eq!(loud.code, 0);
    assert_eq!(loud.stdout, vec![b'o'; 1_048_576]);
    assert_eq!(loud.stderr, vec![b'e'; 1_048_576]);
    let signalled = call(&log, Some("signal"), &args);
    assert_eq!(signalled.code, -15);
    let start = Instant::now();
    let timeout = call(&log, Some("timeout"), &args);
    let elapsed = start.elapsed();
    assert_eq!(timeout.code, 0);
    assert!(
        elapsed >= Duration::from_secs(9),
        "timeout returned too early: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(20),
        "timeout ran too long: {elapsed:?}"
    );
    assert_eq!(fs::read_to_string(log).unwrap().lines().count(), 3);
}
#[test]
fn arbitrary_modes_succeed_and_short_argv_is_logged_before_failure() {
    let temp = tempfile::tempdir().unwrap();
    let log = temp.path().join("sudo.jsonl");
    assert_eq!(
        call(&log, Some("invalid"), &chgrp("/dev/input/event1".into())).code,
        0
    );
    assert_ne!(call(&log, None, &["-n".into()]).code, 0);
    assert_eq!(
        fs::read_to_string(&log).unwrap(),
        "[\"-n\", \"chgrp\", \"gpio\", \"/dev/input/event1\"]\n[\"-n\"]\n"
    );
    assert_eq!(
        call(
            &log,
            Some("fail-weird"),
            &["anything".into(), "weird".into()]
        )
        .code,
        7
    );
    let loud = call(&log, Some("loud"), &[]);
    assert_ne!(loud.code, 0);
    assert_eq!(loud.stdout.len(), 1_048_576);
    assert!(loud.stderr.starts_with(&[b'e'; 1_048_576]));
    assert_eq!(
        &loud.stderr[1_048_576..],
        b"missing second Bluetooth sudo fixture argument\n"
    );
    assert_eq!(fs::read_to_string(&log).unwrap().lines().count(), 4);
    let mut spec = CommandSpec::new_os(
        std::iter::once(OsString::from(binary()))
            .chain(chgrp("/dev/input/event1".into()))
            .collect(),
        Duration::from_secs(20),
    );
    spec.env_remove.push("INPUT_FIXTURE_SUDO_LOG".into());
    assert_ne!(NativeRunner.run(&spec).unwrap().code, 0);
}
#[test]
fn unchanged_python_process_pair_preserves_valid_modes_and_raw_logs() {
    let source = std::env::var_os("BLUETOOTH_SUDO_ORIGINAL_PYTHON")
        .expect("cloud pair requires BLUETOOTH_SUDO_ORIGINAL_PYTHON");
    let source = Path::new(&source);
    assert!(source.is_absolute() && source.is_file());
    let evidence = std::env::var_os("BLUETOOTH_SUDO_EVIDENCE")
        .expect("cloud pair requires BLUETOOTH_SUDO_EVIDENCE");
    let evidence = Path::new(&evidence);
    assert!(evidence.is_absolute());
    fs::create_dir(evidence).expect("Bluetooth sudo evidence directory must be fresh");
    let raw = OsString::from_vec(b"/dev/input/event\xff".to_vec());
    let raw_command = OsString::from_vec(b"cmd\xff".to_vec());
    let raw_mode = OsString::from_vec(b"fail-cmd\xff".to_vec());
    let cases: Vec<(Vec<OsString>, Option<OsString>)> = vec![
        (chgrp(raw), None),
        (chmod("/dev/input/event1".into()), Some("fail-chmod".into())),
        (chgrp("/dev/input/event1".into()), Some("fail-chgrp".into())),
        (
            vec!["arbitrary".into(), "word".into()],
            Some("unknown-mode".into()),
        ),
        (
            vec!["arbitrary".into(), "word".into()],
            Some("fail-word".into()),
        ),
        (vec!["raw".into(), raw_command], Some(raw_mode)),
        (vec!["α".into(), "β".into()], Some("fail-β".into())),
        (vec!["any".into(), "command".into()], Some("loud".into())),
        (vec!["any".into(), "command".into()], Some("signal".into())),
        (vec!["any".into(), "command".into()], Some("timeout".into())),
    ];
    let mut manifest = Vec::new();
    for (index, (args, mode)) in cases.iter().enumerate() {
        let folder = evidence.join(format!("case-{index:02}"));
        fs::create_dir(&folder).unwrap();
        fs::write(
            folder.join("original-executable.json"),
            python_fs::json_string(source.as_os_str()).unwrap(),
        )
        .unwrap();
        fs::write(
            folder.join("native-executable.json"),
            python_fs::json_string(OsStr::new(binary())).unwrap(),
        )
        .unwrap();
        let argv = args
            .iter()
            .map(|arg| python_fs::json_string(arg.as_os_str()).unwrap())
            .collect::<Vec<_>>()
            .join(", ");
        fs::write(folder.join("argv.json"), format!("[{argv}]\n")).unwrap();
        fs::write(
            folder.join("mode.json"),
            mode.as_ref()
                .map(|value| python_fs::json_string(value).unwrap())
                .unwrap_or_else(|| "null".into()),
        )
        .unwrap();
        let original_log = folder.join("original.log");
        let native_log = folder.join("native.log");
        let original_run = pair_child(Some(source), &original_log, mode.as_deref(), args);
        if let Err(error) = &original_run {
            fs::write(folder.join("original.error"), error.to_string()).unwrap();
        }
        let (original, old_time) = original_run.unwrap();
        fs::write(folder.join("original.stdout"), &original.stdout).unwrap();
        fs::write(folder.join("original.stderr"), &original.stderr).unwrap();
        let native_run = pair_child(None, &native_log, mode.as_deref(), args);
        if let Err(error) = &native_run {
            fs::write(folder.join("native.error"), error.to_string()).unwrap();
        }
        let (native, new_time) = native_run.unwrap();
        fs::write(folder.join("native.stdout"), &native.stdout).unwrap();
        fs::write(folder.join("native.stderr"), &native.stderr).unwrap();
        let original_log = fs::read(original_log).unwrap();
        let native_log = fs::read(native_log).unwrap();
        let timing_ok = mode.as_deref() != Some(OsStr::new("timeout"))
            || [old_time, new_time].iter().all(|elapsed| {
                *elapsed >= Duration::from_secs(9) && *elapsed < Duration::from_secs(20)
            });
        let equal = original_log == native_log
            && original.code == native.code
            && original.stdout == native.stdout
            && original.stderr == native.stderr
            && timing_ok;
        let result = json!({
            "case": index,
            "original_code": original.code,
            "native_code": native.code,
            "original_elapsed_ms": old_time.as_millis(),
            "native_elapsed_ms": new_time.as_millis(),
            "logs_equal": original_log == native_log,
            "stdout_equal": original.stdout == native.stdout,
            "stderr_equal": original.stderr == native.stderr,
            "timing_ok": timing_ok,
            "equal": equal
        });
        fs::write(
            folder.join("result.json"),
            serde_json::to_vec_pretty(&result).unwrap(),
        )
        .unwrap();
        manifest.push(result);
        fs::write(
            evidence.join("result-manifest.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }
    assert!(
        manifest.iter().all(|row| row["equal"] == true),
        "Bluetooth sudo process pair differed; inspect persistent evidence"
    );
}
