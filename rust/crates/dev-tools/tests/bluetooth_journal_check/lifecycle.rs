use super::*;
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
};
use std::{
    os::fd::OwnedFd,
    process::{Child, Command, Stdio},
    thread,
    time::Instant,
};
struct Checker {
    child: Child,
    id: OwnedFd,
}
impl Drop for Checker {
    fn drop(&mut self) {
        let _ = pidfd_send_signal(&self.id, Signal::INT);
        let end = Instant::now() + Duration::from_secs(2);
        loop {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            if Instant::now() >= end {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        let _ = pidfd_send_signal(&self.id, Signal::KILL);
        let _ = self.child.wait();
    }
}
struct Children(Vec<OwnedFd>);
impl Drop for Children {
    fn drop(&mut self) {
        for id in &self.0 {
            let _ = pidfd_send_signal(id, Signal::KILL);
        }
    }
}
fn ready_gone(id: &OwnedFd) -> bool {
    poll(
        &mut [PollFd::new(id, PollFlags::IN)],
        Some(&Timespec::default()),
    )
    .unwrap()
        == 1
}
fn lifecycle(interrupt: bool) {
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("candidate");
    let marker = directory.path().join("ready");
    let token = directory.path().file_name().unwrap().to_str().unwrap();
    // Never author an executable while parallel tests can fork and inherit its
    // writable FD. Only the private symlink and environment vary per invocation.
    std::os::unix::fs::symlink(
        repository()
            .join("rust/crates/dev-tools/tests/fixtures/bluetooth-journal/held-candidate.sh"),
        &script,
    )
    .unwrap();
    let output = directory.path().join("output");
    let started = Instant::now();
    let child = Command::new(env!("CARGO_BIN_EXE_check-bluetooth-journal"))
        .arg("--binary")
        .arg(&script)
        .arg("--output")
        .arg(&output)
        .env("OPENPILOT_RUST_ROOT", repository())
        .env("BT_JOURNAL_TOKEN", token)
        .env("BT_JOURNAL_MARKER", &marker)
        .stdin(Stdio::null())
        .stdout(fs::File::create(directory.path().join("stdout")).unwrap())
        .stderr(fs::File::create(directory.path().join("stderr")).unwrap())
        .spawn()
        .unwrap();
    let id = pidfd_open(Pid::from_child(&child), PidfdFlags::empty()).unwrap();
    let mut checker = Checker { child, id };
    let mut children = Children(vec![]);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(text) = fs::read_to_string(&marker) {
            if text.ends_with('\n') {
                let fields = text.split_whitespace().collect::<Vec<_>>();
                if fields.len() == 3 && fields[0] == token {
                    for field in &fields[1..] {
                        let pid = Pid::from_raw(field.parse().unwrap()).unwrap();
                        let id = pidfd_open(pid, PidfdFlags::empty()).unwrap();
                        children.0.push(id); // Admit immediately so any next failure still cleans it.
                        let id = children.0.last().unwrap();
                        let end = Instant::now() + Duration::from_secs(5);
                        // The child may cross exec; identity is held while awaiting
                        // its private inherited environment, never inferred by PID alone.
                        loop {
                            let raw = fs::read(format!("/proc/{}/environ", pid.as_raw_pid()))
                                .unwrap_or_default();
                            if raw
                                .split(|b| *b == 0)
                                .any(|v| v == format!("BT_JOURNAL_TOKEN={token}").as_bytes())
                            {
                                break;
                            }
                            assert!(!ready_gone(id), "child exited before identity ACK");
                            assert!(Instant::now() < end, "private environment ACK absent");
                            thread::sleep(Duration::from_millis(5));
                        }
                    }
                    break;
                }
            }
        }
        assert!(
            checker.child.try_wait().unwrap().is_none(),
            "checker exited before candidate ACK"
        );
        assert!(
            Instant::now() < deadline,
            "candidate did not acknowledge owned children"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(children.0.len(), 2);
    // Only leader SIGINT in the cancellation case; never broadcast a test
    // signal to the nested group. Timeout case uses the real unchanged30s limit.
    if interrupt {
        pidfd_send_signal(&checker.id, Signal::INT).unwrap();
    }
    let end = if interrupt {
        Instant::now() + Duration::from_secs(2)
    } else {
        started + Duration::from_secs(40)
    };
    loop {
        let status = checker.child.try_wait().unwrap();
        if let Some(status) = status {
            assert!(!status.success());
            if children.0.iter().all(ready_gone) {
                break;
            }
        }
        assert!(
            Instant::now() < end,
            "checker or owned descendant survived: interrupt={interrupt}"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert!(!output.join("result.json").exists());
    if !interrupt {
        let error = fs::read_to_string(directory.path().join("stderr")).unwrap();
        assert!(error.contains("timed out"), "wrong failure phase: {error}");
    }
}
#[test]
fn actual_cli_leader_sigint_cleans_acknowledged_candidate_and_descendant() {
    lifecycle(true);
}
#[test]
fn actual_cli_thirty_second_deadline_cleans_acknowledged_candidate_and_descendant() {
    lifecycle(false);
}
