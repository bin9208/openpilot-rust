use crate::Error;
use std::{fs, time::Duration};

pub(crate) fn command(pid: i32) -> String {
    fs::read(format!("/proc/{pid}/cmdline"))
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default()
}
pub(crate) fn alive(pid: i32, pattern: &str) -> bool {
    rustix::process::Pid::from_raw(pid)
        .is_some_and(|pid| rustix::process::test_kill_process(pid).is_ok())
        && (pattern.is_empty() || command(pid).contains(pattern))
}
pub(crate) fn matching(pattern: &str) -> Vec<i32> {
    let mut pids = Vec::new();
    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            if let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<i32>().ok())
            {
                if command(pid).contains(pattern) {
                    pids.push(pid);
                }
            }
        }
    }
    pids.sort_unstable();
    pids
}
pub(crate) async fn terminate(pid: i32, pattern: &str, timeout: Duration) -> Result<(), Error> {
    if !alive(pid, pattern) {
        return Ok(());
    }
    let Some(native) = rustix::process::Pid::from_raw(pid) else {
        return Ok(());
    };
    if rustix::process::kill_process(native, rustix::process::Signal::TERM).is_err() {
        return Ok(());
    }
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        if !alive(pid, pattern) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if alive(pid, pattern) {
        let _killed = rustix::process::kill_process(native, rustix::process::Signal::KILL);
    }
    Ok(())
}
