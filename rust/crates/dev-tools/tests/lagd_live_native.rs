//! Actual POSIX child/pipe ownership without any candidate or Python execution.
#![cfg(feature = "lagd-tools-native")]
use openpilot_dev_tools::lagd_live_check::{NativeRuntime, Runtime, Spawn};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    time::{Duration, Instant},
};
fn gone_or_zombie(pid: u32) -> bool {
    match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(text) => text
            .rsplit_once(')')
            .is_some_and(|(_, tail)| tail.trim_start().starts_with('Z')),
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    }
}
fn specification(command: &str) -> Spawn {
    Spawn {
        argv: vec!["/bin/sh".into(), "-c".into(), command.into()],
        env: BTreeMap::new(),
        stdin_pipe: false,
        stdout: None,
        stderr: None,
        stdout_file: None,
        stderr_file: None,
        new_session: true,
    }
}
#[test]
fn exited_leader_remains_owned_until_group_cleanup_and_cannot_be_signalled_after_reap() {
    let mut native = NativeRuntime::new().unwrap();
    let child = native
        .spawn(&specification(r#"sleep 20 & printf '%s %s\n' "$$" "$!""#))
        .unwrap();
    let line = native
        .read_line(child, Some(Duration::from_secs(3)))
        .unwrap();
    let pids = std::str::from_utf8(&line)
        .unwrap()
        .split_whitespace()
        .map(|v| v.parse::<u32>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(pids.len(), 2);
    assert_eq!(native.wait(child, Duration::from_secs(3)).unwrap(), 0);
    assert!(
        Path::new(&format!("/proc/{}", pids[0])).exists(),
        "WNOWAIT reserves the exact leader until owned group cleanup"
    );
    native.close(child).unwrap();
    let end = Instant::now() + Duration::from_secs(2);
    while !gone_or_zombie(pids[1]) && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        gone_or_zombie(pids[1]),
        "descendant survived owned group cleanup"
    );
    assert!(!Path::new(&format!("/proc/{}", pids[0])).exists());
    assert!(native.signal(child, 15).is_err());
    assert!(native.poll(child).is_err());
    assert!(native.wait(child, Duration::from_secs(1)).is_err());
}
#[test]
fn actual_pipe_deadline_closes_live_descendants_and_log_open_precedes_spawn() {
    let temp = tempfile::tempdir().unwrap();
    let mut native = NativeRuntime::new().unwrap();
    let marker = temp.path().join("must-not-spawn");
    let mut failed = specification(r#"printf forbidden > "$0""#);
    failed.argv.push(marker.as_os_str().to_owned());
    failed.stderr = Some(temp.path().to_owned());
    assert!(native.spawn(&failed).is_err());
    assert!(!marker.exists());
    let child = native
        .spawn(&specification(r#"sleep 20 & printf '%s\n' "$!"; wait"#))
        .unwrap();
    let line = native
        .read_line(child, Some(Duration::from_secs(3)))
        .unwrap();
    let descendant = std::str::from_utf8(&line)
        .unwrap()
        .trim()
        .parse::<u32>()
        .unwrap();
    let start = Instant::now();
    assert!(native
        .read_line(child, Some(Duration::from_millis(30)))
        .is_err());
    assert!(start.elapsed() < Duration::from_secs(2));
    native.close(child).unwrap();
    let end = Instant::now() + Duration::from_secs(2);
    while !gone_or_zombie(descendant) && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(gone_or_zombie(descendant));
}
