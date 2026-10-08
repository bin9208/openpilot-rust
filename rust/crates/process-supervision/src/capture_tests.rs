use super::*;
use std::{
    os::unix::process::ExitStatusExt,
    process::{Command, Stdio},
};

fn child(script: &str) -> Child {
    Command::new("/bin/sh")
        .args(["-c", script])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}
#[test]
fn separate_raw_streams_and_nonzero_status() {
    let mut child = child("printf '\\377owned\\r\\n'; printf 'stderr\\r' >&2; exit 7");
    let output = capture_output(&mut child, Duration::from_secs(2)).unwrap();
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(output.stdout, b"\xffowned\r\n");
    assert_eq!(output.stderr, b"stderr\r");
}
#[test]
fn failed_spawn_has_typed_launch_error() {
    let directory = tempfile::tempdir().unwrap();
    let command = crate::CapturedCommand {
        launcher: directory.path().join("missing-owned-launcher"),
        cwd: directory.path().into(),
        argv: vec!["/bin/true".into()],
    };
    assert!(matches!(
        command.capture(Duration::from_secs(1), None),
        Err(CaptureError::Launch(crate::Error::Io(_)))
    ));
}
#[test]
fn signal_status_preserved() {
    let mut child = child("kill -TERM $$");
    let output = capture_output(&mut child, Duration::from_secs(2)).unwrap();
    assert_eq!(output.status.signal(), Some(15));
}
#[test]
fn descendant_pipe_eof_obeys_total_deadline_without_group_kill() {
    let directory = tempfile::tempdir().unwrap();
    let pid = directory.path().join("owned-holder-pid");
    let script = format!(
        "sleep 2 & printf '%s' $! > '{}'; printf owned; exit 0",
        pid.display()
    );
    let mut child = child(&script);
    let started = Instant::now();
    let result = capture_output(&mut child, Duration::from_millis(100));
    assert!(started.elapsed() < Duration::from_secs(1));
    match result {
        Err(CaptureError::Timeout { stdout, .. }) => assert_eq!(stdout, b"owned"),
        other => panic!("unexpected capture {other:?}"),
    }
    assert_eq!(child.try_wait().unwrap().unwrap().code(), Some(0));
    let holder = std::fs::read_to_string(pid)
        .unwrap()
        .parse::<i32>()
        .unwrap();
    assert!(std::fs::read_to_string(format!("/proc/{holder}/stat")).is_ok());
    let holder = rustix::process::Pid::from_raw(holder).unwrap();
    let _ = rustix::process::kill_process(holder, rustix::process::Signal::TERM);
}
#[test]
fn child_with_closed_pipes_still_obeys_deadline() {
    let mut child = child("exec 1>&- 2>&-; exec sleep 2");
    let started = Instant::now();
    assert!(matches!(
        capture_output(&mut child, Duration::from_millis(100)),
        Err(CaptureError::Timeout { .. })
    ));
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(child.try_wait().unwrap().is_some());
}
#[test]
fn missing_pipe_setup_reaps_direct_child() {
    let mut child = Command::new("/bin/sh")
        .args(["-c", "exec sleep 2"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    assert!(matches!(
        capture_output(&mut child, Duration::from_secs(1)),
        Err(CaptureError::Io(_))
    ));
    assert!(child.try_wait().unwrap().is_some());
    assert!(child.stderr.is_none());
}
