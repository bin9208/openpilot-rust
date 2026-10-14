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
