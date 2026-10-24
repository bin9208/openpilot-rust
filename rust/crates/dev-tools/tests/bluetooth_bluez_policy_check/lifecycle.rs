use super::*;
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
};
use std::{
    os::fd::OwnedFd,
    process::{Child, Stdio},
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
        while self.child.try_wait().ok().flatten().is_none() && Instant::now() < end {
            thread::sleep(Duration::from_millis(5));
        }
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = pidfd_send_signal(&self.id, Signal::KILL);
            let _ = self.child.wait();
        }
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
fn gone(id: &OwnedFd) -> bool {
    poll(
        &mut [PollFd::new(id, PollFlags::IN)],
        Some(&Timespec::default()),
    )
    .unwrap()
        == 1
}

#[test]
fn sigint_cleans_held_candidate_and_descendant_without_receipt() {
    let directory = tempfile::tempdir().unwrap();
    let candidate = directory.path().join("candidate");
    let marker = directory.path().join("ready");
    let token = directory.path().file_name().unwrap().to_str().unwrap();
    std::os::unix::fs::symlink(
        root().join("rust/crates/dev-tools/tests/fixtures/bluetooth-config/held-candidate.sh"),
        &candidate,
    )
    .unwrap();
    let output = directory.path().join("output");
    let child = Command::new(env!("CARGO_BIN_EXE_check-bluetooth-bluez-policy"))
        .arg("--binary")
        .arg(&candidate)
        .arg("--output")
        .arg(&output)
        .env("OPENPILOT_RUST_ROOT", root())
        .env("BT_CONFIG_TOKEN", token)
        .env("BT_CONFIG_MARKER", &marker)
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
                        children
                            .0
                            .push(pidfd_open(pid, PidfdFlags::empty()).unwrap());
                        let id = children.0.last().unwrap();
                        let ack = Instant::now() + Duration::from_secs(5);
                        loop {
                            let env = fs::read(format!("/proc/{}/environ", pid.as_raw_pid()))
                                .unwrap_or_default();
                            if env
                                .split(|byte| *byte == 0)
                                .any(|item| item == format!("BT_CONFIG_TOKEN={token}").as_bytes())
                            {
                                break;
                            }
                            assert!(!gone(id), "held candidate exited before identity ACK");
                            assert!(Instant::now() < ack, "private environment ACK absent");
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
            "held candidate did not acknowledge children"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(children.0.len(), 2);
    pidfd_send_signal(&checker.id, Signal::INT).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(status) = checker.child.try_wait().unwrap() {
            assert!(!status.success());
            if children.0.iter().all(gone) {
                break;
            }
        }
        assert!(
            Instant::now() < deadline,
            "checker or candidate survived SIGINT"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert!(!output.join("result.json").exists());
}
