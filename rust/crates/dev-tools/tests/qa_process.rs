use openpilot_dev_tools::qa_process::{CaptureError, CommandSpec, NativeRunner, Runner};
use std::{
    fs,
    time::{Duration, Instant},
};
fn command(script: &str, timeout: Duration) -> CommandSpec {
    CommandSpec::new(vec!["/bin/sh".into(), "-c".into(), script.into()], timeout)
}
fn gone_or_zombie(pid: i32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/stat")).map_or(true, |v| {
        v.rsplit_once(')')
            .is_some_and(|(_, s)| s.trim_start().starts_with('Z'))
    })
}
fn assert_terminated(pid: i32) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while !gone_or_zombie(pid) {
        assert!(
            Instant::now() < deadline,
            "owned descendant {pid} survived group termination"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn raw_separate_streams_and_nonzero_status_are_preserved() {
    let r = NativeRunner
        .run(&command(
            "printf '\\377owned\\r\\n'; printf 'error\\r' >&2; exit 7",
            Duration::from_secs(2),
        ))
        .unwrap();
    assert_eq!(r.code, 7);
    assert_eq!(r.stdout, b"\xffowned\r\n");
    assert_eq!(r.stderr, b"error\r");
}
#[test]
fn merged_streams_keep_write_order_raw_bytes_and_exit_status() {
    let mut spec = command(
        "printf 'first\\377'; printf 'second\\r' >&2; printf 'third\\n'; printf 'last' >&2; exit 7",
        Duration::from_secs(2),
    );
    spec.merge_output = true;
    let result = NativeRunner.run(&spec).unwrap();
    assert_eq!(result.code, 7);
    assert_eq!(result.stdout, b"first\xffsecond\rthird\nlast");
    assert!(result.stderr.is_empty());
}

#[test]
fn merged_timeout_keeps_partial_ordered_output_and_stops_owned_descendants() {
    let temp = tempfile::tempdir().unwrap();
    let pidfile = temp.path().join("worker.pid");
    let mut spec = command(
        &format!(
            "printf 'one'; printf 'two' >&2; sleep 30 & printf '%s' $! > '{}'; wait",
            pidfile.display()
        ),
        Duration::from_millis(200),
    );
    spec.merge_output = true;
    let error = NativeRunner.run(&spec).unwrap_err();
    let Some(CaptureError::Timeout { stdout, stderr, .. }) = error.downcast_ref::<CaptureError>()
    else {
        panic!("expected merged timeout: {error}");
    };
    assert_eq!(stdout, b"onetwo");
    assert!(stderr.is_empty());
    assert_terminated(fs::read_to_string(pidfile).unwrap().parse().unwrap());
}

#[test]
fn inherited_stream_child() {
    if std::env::var_os("OPENPILOT_QA_INHERITED_STREAM_CHILD").is_none() {
        return;
    }
    let mut spec = command(
        "printf 'inherited\\377'; printf 'diagnostic\\r' >&2; exit 7",
        Duration::from_secs(2),
    );
    spec.inherit_output = true;
    let result = NativeRunner.run(&spec).unwrap();
    assert_eq!(result.code, 7);
    assert!(result.stdout.is_empty());
    assert!(result.stderr.is_empty());
}

#[test]
fn inherited_streams_reach_the_caller_without_capture_or_translation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "inherited_stream_child", "--nocapture"])
        .env("OPENPILOT_QA_INHERITED_STREAM_CHILD", "1")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output
        .stdout
        .windows(b"inherited\xff".len())
        .any(|value| value == b"inherited\xff"));
    assert_eq!(output.stderr, b"diagnostic\r");
}

#[test]
fn incompatible_output_modes_fail_before_spawn_and_inherited_timeouts_stop_groups() {
    let mut spec = command("exit 99", Duration::from_secs(2));
    spec.inherit_output = true;
    spec.merge_output = true;
    assert!(NativeRunner.run(&spec).is_err());
    spec.merge_output = false;
    let temp = tempfile::tempdir().unwrap();
    let pidfile = temp.path().join("worker.pid");
    spec.argv[2] = format!("sleep 30 & printf '%s' $! > '{}'; wait", pidfile.display());
    spec.timeout = Duration::from_millis(200);
    let error = NativeRunner.run(&spec).unwrap_err();
    assert!(matches!(
        error.downcast_ref::<CaptureError>(),
        Some(CaptureError::Timeout { .. })
    ));
    assert_terminated(fs::read_to_string(pidfile).unwrap().parse().unwrap());
}
#[test]
fn timeout_kills_owned_descendants_and_reaps_leader() {
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("child.pid");
    let script = format!("sleep 30 & printf '%s' $! > '{}'; wait", pidfile.display());
    let start = Instant::now();
    let error = NativeRunner
        .run(&command(&script, Duration::from_millis(200)))
        .unwrap_err();
    assert!(matches!(
        error.downcast_ref::<CaptureError>(),
        Some(CaptureError::Timeout { .. })
    ));
    assert!(start.elapsed() < Duration::from_secs(2));
    let pid = fs::read_to_string(pidfile).unwrap().parse().unwrap();
    assert_terminated(pid);
}
#[test]
fn successful_leader_cannot_leave_background_workers_or_pipe_holders() {
    let r = NativeRunner
        .run(&command(
            "sleep 30 & printf '%s' $!; exit 0",
            Duration::from_secs(2),
        ))
        .unwrap();
    assert_eq!(r.code, 0);
    let pid = String::from_utf8(r.stdout).unwrap().parse().unwrap();
    assert_terminated(pid);
}
#[test]
fn both_stream_limits_fail_closed_and_stop_the_group() {
    for stderr in [false, true] {
        let mut spec = command(
            if stderr { "yes owned >&2" } else { "yes owned" },
            Duration::from_secs(2),
        );
        spec.stdout_limit = 1024;
        spec.stderr_limit = 1024;
        let error = NativeRunner.run(&spec).unwrap_err();
        assert!(
            matches!(error.downcast_ref::<CaptureError>(),Some(CaptureError::Limit{stream,..})if *stream==if stderr{"stderr"}else{"stdout"})
        );
    }
}
#[test]
fn failed_spawn_closed_pipes_and_signal_status_are_bounded() {
    let mut spec = command("exec 1>&- 2>&-; sleep 30", Duration::from_millis(100));
    assert!(NativeRunner
        .run(&spec)
        .unwrap_err()
        .downcast_ref::<CaptureError>()
        .is_some());
    spec.argv = vec!["/missing-owned-native-command".into()];
    assert!(NativeRunner.run(&spec).is_err());
    let r = NativeRunner
        .run(&command("kill -TERM $$", Duration::from_secs(2)))
        .unwrap();
    assert_eq!(r.code, -15);
}

#[test]
fn file_backed_logs_preserve_large_raw_output_order_without_capture_limits() {
    let temp = tempfile::tempdir().unwrap();
    let log = temp.path().join("compiler.log");
    let mut spec = command(
        "printf 'first\\377'; head -c 17825792 /dev/zero; printf 'last\\r' >&2; exit 7",
        Duration::from_secs(5),
    );
    spec.stdout_limit = 1;
    spec.stderr_limit = 1;
    let captured = NativeRunner.run_to_file(&spec, &log).unwrap();
    assert_eq!(captured.code, 7);
    assert!(captured.stdout.is_empty() && captured.stderr.is_empty());
    let bytes = fs::read(log).unwrap();
    assert_eq!(bytes.len(), 17_825_792 + 11);
    assert_eq!(&bytes[..6], b"first\xff");
    assert!(bytes[6..bytes.len() - 5].iter().all(|byte| *byte == 0));
    assert_eq!(&bytes[bytes.len() - 5..], b"last\r");
}

#[test]
fn file_backed_timeout_preserves_partial_log_and_reaps_owned_workers() {
    let temp = tempfile::tempdir().unwrap();
    let log = temp.path().join("compiler.log");
    let pidfile = temp.path().join("worker.pid");
    let spec = command(
        &format!(
            "printf 'partial\\377'; printf 'error' >&2; sleep 30 & printf '%s' $! > '{}'; wait",
            pidfile.display()
        ),
        Duration::from_millis(200),
    );
    let error = NativeRunner.run_to_file(&spec, &log).unwrap_err();
    assert!(matches!(
        error.downcast_ref::<CaptureError>(),
        Some(CaptureError::Timeout { .. })
    ));
    assert_eq!(fs::read(log).unwrap(), b"partial\xfferror");
    assert_terminated(fs::read_to_string(pidfile).unwrap().parse().unwrap());
}

#[test]
fn file_backed_spawn_failure_truncates_owned_log_and_bad_log_prevents_execution() {
    let temp = tempfile::tempdir().unwrap();
    let log = temp.path().join("compiler.log");
    fs::write(&log, b"stale success").unwrap();
    let mut spec = command("exit 0", Duration::from_secs(2));
    spec.argv = vec!["/missing-owned-native-command".into()];
    assert!(NativeRunner.run_to_file(&spec, &log).is_err());
    assert!(fs::read(log).unwrap().is_empty());
    let marker = temp.path().join("spawned");
    let spec = command(
        &format!("touch '{}'", marker.display()),
        Duration::from_secs(2),
    );
    assert!(NativeRunner.run_to_file(&spec, temp.path()).is_err());
    assert!(!marker.exists());
}

#[test]
fn piped_input_is_fed_while_both_output_streams_are_drained() {
    let mut spec = command(
        "head -c 1048576 /dev/zero; head -c 1048576 /dev/zero >&2; cat; printf 'done' >&2",
        Duration::from_secs(5),
    );
    let input = (0..1_048_576).map(|i| (i % 256) as u8).collect::<Vec<_>>();
    spec.input = Some(input.clone());
    let result = NativeRunner.run(&spec).unwrap();
    assert_eq!(result.code, 0);
    assert_eq!(result.stdout.len(), 2_097_152);
    assert!(result.stdout[..1_048_576].iter().all(|&byte| byte == 0));
    assert_eq!(&result.stdout[1_048_576..], input);
    assert_eq!(result.stderr.len(), 1_048_580);
    assert_eq!(&result.stderr[1_048_576..], b"done");
}

#[test]
fn empty_and_early_closed_input_preserve_eof_and_child_status() {
    let mut spec = command("cat; printf 'eof'", Duration::from_secs(2));
    spec.input = Some(vec![]);
    let result = NativeRunner.run(&spec).unwrap();
    assert_eq!(result.stdout, b"eof");
    assert_eq!(result.code, 0);
    spec.argv[2] = "exec 0<&-; printf 'closed'; exit 7".into();
    spec.input = Some(vec![42; 1_048_576]);
    let result = NativeRunner.run(&spec).unwrap();
    assert_eq!(result.stdout, b"closed");
    assert_eq!(result.code, 7);
}

#[test]
fn blocked_input_remains_subject_to_deadline_and_group_cleanup() {
    let temp = tempfile::tempdir().unwrap();
    let pidfile = temp.path().join("worker.pid");
    let mut spec = command(
        &format!(
            "printf 'waiting'; sleep 30 & printf '%s' $! > '{}'; wait",
            pidfile.display()
        ),
        Duration::from_millis(200),
    );
    spec.input = Some(vec![42; 1_048_576]);
    let error = NativeRunner.run(&spec).unwrap_err();
    let Some(CaptureError::Timeout { stdout, .. }) = error.downcast_ref::<CaptureError>() else {
        panic!("expected timeout while input is blocked: {error}");
    };
    assert_eq!(stdout, b"waiting");
    assert_terminated(fs::read_to_string(pidfile).unwrap().parse().unwrap());
}

#[test]
fn inherited_stderr_child() {
    if std::env::var_os("OPENPILOT_QA_INHERITED_STDERR_CHILD").is_none() {
        return;
    }
    let mut spec = command(
        "printf 'captured\\377'; printf 'inherited-error\\r' >&2; exit 7",
        Duration::from_secs(2),
    );
    spec.inherit_stderr = true;
    let result = NativeRunner.run(&spec).unwrap();
    assert_eq!(result.code, 7);
    assert_eq!(result.stdout, b"captured\xff");
    assert!(result.stderr.is_empty());
}

#[test]
fn separately_inherited_stderr_keeps_stdout_captured() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "inherited_stderr_child", "--nocapture"])
        .env("OPENPILOT_QA_INHERITED_STDERR_CHILD", "1")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!output
        .stdout
        .windows(b"captured\xff".len())
        .any(|v| v == b"captured\xff"));
    assert_eq!(output.stderr, b"inherited-error\r");
    let mut spec = command("exit 99", Duration::from_secs(2));
    spec.inherit_stderr = true;
    spec.merge_output = true;
    assert!(NativeRunner.run(&spec).is_err());
}

#[test]
fn inherited_stdin_child() {
    if std::env::var_os("OPENPILOT_QA_INHERITED_STDIN_CHILD").is_none() {
        return;
    }
    let mut spec = command("cat", Duration::from_secs(2));
    spec.inherit_stdin = true;
    if std::env::var_os("OPENPILOT_QA_PREFIX_CHILD").is_some() {
        let temp = tempfile::tempdir().unwrap();
        let log = temp.path().join("inherited.log");
        NativeRunner
            .run_to_file_with_prefix(&spec, &log, b"Invocation\n")
            .unwrap();
        assert_eq!(fs::read(log).unwrap(), b"Invocation\ninherited\x00\xff\r\n");
    } else {
        let result = NativeRunner.run(&spec).unwrap();
        assert_eq!(result.stdout, b"inherited\x00\xff\r\n");
    }
    spec.input = Some(vec![]);
    assert!(NativeRunner.run(&spec).is_err());
}
#[test]
fn inherited_stdin_reaches_child_without_translation() {
    use std::io::Write;
    for prefixed in [false, true] {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        if prefixed {
            command.env("OPENPILOT_QA_PREFIX_CHILD", "1");
        }
        let mut child = command
            .args(["--exact", "inherited_stdin_child", "--nocapture"])
            .env("OPENPILOT_QA_INHERITED_STDIN_CHILD", "1")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"inherited\x00\xff\r\n")
            .unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(result.status.success(), "{:?}", result);
    }
}

#[test]
fn prefixed_file_keeps_raw_argv_environment_and_ordered_output() {
    use std::{collections::BTreeMap, ffi::OsString, os::unix::ffi::OsStringExt};
    let temp = tempfile::tempdir().unwrap();
    let log = temp.path().join("compiler.log");
    let mut spec = CommandSpec::new_os(
        vec![
            "/bin/sh".into(),
            "-c".into(),
            "printf '%s' \"$1\"; printf '%s' \"$QA_RAW_VALUE\" >&2; cat".into(),
            "peer".into(),
            OsString::from_vec(b"arg-\xff".to_vec()),
        ],
        Duration::from_secs(2),
    );
    spec.set_os_env(BTreeMap::from([(
        "QA_RAW_VALUE".into(),
        OsString::from_vec(b"env-\xfe".to_vec()),
    )]));
    spec.env
        .insert("QA_RAW_VALUE".into(), "must-not-replace".into());
    spec.input = Some(b"input\0\xfd".to_vec());
    spec.inherit_output = true;
    spec.inherit_stderr = true;
    let result = NativeRunner
        .run_to_file_with_prefix(&spec, &log, b"Invocation: peer\n")
        .unwrap();
    assert_eq!(result.code, 0);
    assert!(result.stdout.is_empty() && result.stderr.is_empty());
    assert_eq!(
        fs::read(log).unwrap(),
        b"Invocation: peer\narg-\xffenv-\xfeinput\0\xfd"
    );
}

#[test]
fn prefixed_spawn_failure_and_timeout_preserve_owned_diagnostics() {
    let temp = tempfile::tempdir().unwrap();
    let log = temp.path().join("compiler.log");
    fs::write(&log, b"stale success").unwrap();
    let spec = CommandSpec::new(
        vec!["/missing-owned-command".into()],
        Duration::from_secs(1),
    );
    assert!(NativeRunner
        .run_to_file_with_prefix(&spec, &log, b"Invocation: missing\n")
        .is_err());
    assert_eq!(fs::read(&log).unwrap(), b"Invocation: missing\n");
    let spec = command(
        "printf 'partial\\377'; printf error >&2; sleep 30",
        Duration::from_millis(200),
    );
    let error = NativeRunner
        .run_to_file_with_prefix(&spec, &log, b"Invocation: timeout\n")
        .unwrap_err();
    assert!(matches!(
        error.downcast_ref::<CaptureError>(),
        Some(CaptureError::Timeout { .. })
    ));
    assert_eq!(
        fs::read(log).unwrap(),
        b"Invocation: timeout\npartial\xfferror"
    );
}

#[test]
fn authoritative_empty_environment_and_invalid_environment_preserve_defaults() {
    use std::{collections::BTreeMap, ffi::OsString, os::unix::ffi::OsStringExt};
    let mut spec = command(
        "printf '%s:%s' \"${OPENPILOT_QA_IGNORED_OVERLAY-unset}\" \"${PATH:+inherited}\"",
        Duration::from_secs(2),
    );
    spec.set_os_env(BTreeMap::new());
    spec.env
        .insert("OPENPILOT_QA_IGNORED_OVERLAY".into(), "wrong".into());
    assert_eq!(NativeRunner.run(&spec).unwrap().stdout, b"unset:inherited");
    spec.set_os_env(BTreeMap::from([("QA_VALUE".into(), "valid".into())]));
    assert_eq!(spec.env.get("QA_VALUE").map(String::as_str), Some("valid"));
    spec.set_os_env(BTreeMap::from([(
        "QA_VALUE".into(),
        OsString::from_vec(b"bad\0value".to_vec()),
    )]));
    assert!(NativeRunner.run(&spec).is_err());
}

#[test]
fn default_peer_prefix_survives_success_and_timeout() {
    use openpilot_dev_tools::{qa_process::Captured, seal::Result};
    struct Peer(bool);
    impl Runner for Peer {
        fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
            assert!(spec.merge_output && spec.inherit_stdin);
            assert!(!spec.inherit_output && !spec.inherit_stderr);
            if self.0 {
                Err(CaptureError::Timeout {
                    timeout: spec.timeout,
                    stdout: b"partial\xff".to_vec(),
                    stderr: vec![],
                }
                .into())
            } else {
                Ok(Captured {
                    code: 7,
                    stdout: b"complete\xff".to_vec(),
                    stderr: vec![],
                })
            }
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let log = temp.path().join("peer.log");
    let mut spec = command("unused", Duration::from_secs(1));
    spec.inherit_stdin = true;
    spec.inherit_output = true;
    spec.inherit_stderr = true;
    assert_eq!(
        Peer(false)
            .run_to_file_with_prefix(&spec, &log, b"prefix\n")
            .unwrap()
            .code,
        7
    );
    assert_eq!(fs::read(&log).unwrap(), b"prefix\ncomplete\xff");
    assert!(Peer(true)
        .run_to_file_with_prefix(&spec, &log, b"prefix\n")
        .is_err());
    assert_eq!(fs::read(log).unwrap(), b"prefix\npartial\xff");
}
