//! Owned interactive child: signals, leader observation and descendant cleanup.
use openpilot_dev_tools::{qa_process::CommandSpec, timed_live_child::Child};
use rustix::process::Signal;
use std::{
    fs,
    path::Path,
    thread,
    time::{Duration, Instant},
};

fn running(pid: u32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|stat| {
            stat.rsplit_once(") ")
                .map(|(_, fields)| fields.starts_with('Z'))
        })
        .is_some_and(|zombie| !zombie)
}
fn wait_for(mut condition: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(3);
    while !condition() {
        assert!(
            Instant::now() < until,
            "owned child did not reach expected phase"
        );
        thread::sleep(Duration::from_millis(2));
    }
}
fn shell(script: &str) -> CommandSpec {
    CommandSpec::new(
        vec!["/bin/sh".into(), "-c".into(), script.into()],
        Duration::from_secs(5),
    )
}
#[test]
fn exited_leader_remains_observable_until_descendants_are_cleaned() {
    for wait in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("child.log");
        let mut child = Child::spawn(&shell("/bin/sleep 30 & echo $!; exit 0"), &output).unwrap();
        let leader = child.id();
        wait_for(|| child.poll().unwrap() == Some(0));
        // poll must not release the group leader's PID while its child is live.
        assert!(Path::new(&format!("/proc/{leader}/stat")).exists());
        let descendant = fs::read_to_string(&output)
            .unwrap()
            .trim()
            .parse::<u32>()
            .unwrap();
        assert!(running(descendant));
        if wait {
            assert_eq!(child.wait(Duration::from_secs(3)).unwrap(), 0);
            assert_eq!(child.poll().unwrap(), Some(0));
            // No stale PID/group access after reap; repeated lifecycle calls
            // use the recorded outcome and cannot signal a recycled group.
            child.signal(Signal::TERM).unwrap();
            child.kill().unwrap();
            assert_eq!(child.wait(Duration::from_secs(3)).unwrap(), 0);
        }
        drop(child);
        wait_for(|| !running(descendant));
        assert!(!Path::new(&format!("/proc/{leader}")).exists());
    }
}
#[test]
fn actual_signal_timeout_input_and_open_failure_boundaries() {
    let directory = tempfile::tempdir().unwrap();
    let mut spec = shell("IFS= read -r value; printf '%s' \"$value\"");
    spec.input = Some(b"payload\n".to_vec());
    let output = directory.path().join("input.log");
    let mut child = Child::spawn(&spec, &output).unwrap();
    assert_eq!(child.wait(Duration::from_secs(3)).unwrap(), 0);
    assert_eq!(fs::read(output).unwrap(), b"payload");
    let mut child = Child::spawn(
        &shell("exec /bin/sleep 30"),
        &directory.path().join("signal.log"),
    )
    .unwrap();
    child.signal(Signal::TERM).unwrap();
    assert_eq!(child.wait(Duration::from_secs(3)).unwrap(), -15);
    let mut child = Child::spawn(
        &shell("exec /bin/sleep 30"),
        &directory.path().join("timeout.log"),
    )
    .unwrap();
    let pid = child.id();
    assert!(child.wait(Duration::from_millis(20)).is_err());
    drop(child);
    assert!(!running(pid));
    assert!(Child::spawn(&shell("exit 0"), directory.path()).is_err());
    let output = directory.path().join("missing.log");
    assert!(Child::spawn(
        &CommandSpec::new(vec!["/missing/timed-child".into()], Duration::from_secs(3)),
        &output
    )
    .is_err());
    assert_eq!(fs::read(output).unwrap(), b"");
}
