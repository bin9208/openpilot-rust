//! Actual CommandChild cancellation over private helpers, never host sudo/NM.
use super::*;
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
};
use std::{
    fs,
    io::Write,
    os::{fd::OwnedFd, unix::fs::DirBuilderExt},
    path::{Path, PathBuf},
    time::Instant,
};

struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
// Safety net for failures before the readiness record has been parsed. Only
// children carrying this test's unique initial environment are considered; the
// same nonce is rechecked after pidfd_open before any signal is sent.
struct NonceCleanup(String);
impl Drop for NonceCleanup {
    fn drop(&mut self) {
        let expected = format!("WIFI_CHILD_NONCE={}", self.0);
        let Ok(entries) = fs::read_dir("/proc") else {
            return;
        };
        for entry in entries.flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|v| v.parse::<i32>().ok())
                .and_then(Pid::from_raw)
            else {
                continue;
            };
            let same_owner = || {
                fs::read(entry.path().join("environ")).is_ok_and(|env| {
                    env.split(|byte| *byte == 0)
                        .any(|part| part == expected.as_bytes())
                })
            };
            if !same_owner() {
                continue;
            }
            let Ok(identity) = pidfd_open(pid, PidfdFlags::empty()) else {
                continue;
            };
            if same_owner() {
                let _ = pidfd_send_signal(&identity, Signal::KILL);
            }
        }
    }
}
struct Identity(OwnedFd);
impl Identity {
    fn open(pid: Pid) -> Self {
        Self(pidfd_open(pid, PidfdFlags::empty()).expect("Linux pidfd prerequisite"))
    }
    fn exited(&self) -> bool {
        let mut fds = [PollFd::new(&self.0, PollFlags::IN)];
        poll(
            &mut fds,
            Some(&Timespec {
                tv_sec: 0,
                tv_nsec: 0,
            }),
        )
        .unwrap();
        fds[0].revents().contains(PollFlags::IN)
    }
}
impl Drop for Identity {
    fn drop(&mut self) {
        // Fallback test cleanup occurs AFTER the survival assertion. A held
        // pidfd can never signal a reused numeric PID or an unrelated group.
        let _ = pidfd_send_signal(&self.0, Signal::KILL);
    }
}
fn wait_for(condition: impl Fn() -> bool, description: &str) {
    let until = Instant::now() + Duration::from_secs(3);
    while !condition() {
        assert!(Instant::now() < until, "{description}");
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn release(path: &Path, command: &str) {
    use rustix::fs::{open, Mode, OFlags};
    let until = Instant::now() + Duration::from_secs(3);
    let fd = loop {
        match open(
            path,
            OFlags::WRONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(fd) => break fd,
            Err(rustix::io::Errno::NXIO) => {
                assert!(Instant::now() < until, "helper control FIFO readiness");
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("private control FIFO: {error}"),
        }
    };
    writeln!(fs::File::from(fd), "{command}").unwrap();
}
fn scenario(mode: &str) {
    let leader_exits = mode == "exit";
    let launcher = PathBuf::from(
        std::env::var_os("WIFI_PROCESS_CHILD_BINARY")
            .expect("prerequisite: WIFI_PROCESS_CHILD_BINARY names fresh openpilot-process-child"),
    );
    assert!(launcher.is_absolute() && launcher.is_file());
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let directory = Directory(std::env::temp_dir().join(format!("wifi-child-owner-{nonce}")));
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory.0)
        .unwrap();
    let helper = directory.0.join("private-sudo.sh");
    let ready = directory.0.join("ready");
    let control = directory.0.join("control");
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        &control,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .unwrap();
    fs::write(&helper, b"#!/bin/sh\nset -eu\n[ \"$1\" = sysctl ]\n[ \"$2\" = net.ipv4.ip_forward=0 ]\nexport WIFI_CHILD_NONCE=\"$4\"\n/bin/sleep 120 &\nsleeper=$!\nprintf '%s %s %s\\n' \"$$\" \"$sleeper\" \"$4\" > \"$3.pending\"\n/bin/mv -- \"$3.pending\" \"$3\"\nIFS= read -r mode < \"$5\"\nif [ \"$mode\" = exit ]; then exit 0; fi\nif [ \"$mode\" = complete ]; then kill \"$sleeper\"; wait \"$sleeper\" || :; exit 17; fi\nwait \"$sleeper\"\n").unwrap();
    let _cleanup = NonceCleanup(nonce.clone());
    let command = CapturedCommand {
        launcher,
        cwd: directory.0.clone(),
        argv: vec![
            "/bin/sh".into(),
            helper.into_os_string(),
            "sysctl".into(),
            "net.ipv4.ip_forward=0".into(),
            ready.clone().into_os_string(),
            nonce.clone().into(),
            control.clone().into_os_string(),
        ],
    };
    // This uses the actual inherited-session launcher and production Drop path.
    // The child-only nonce adds no parent/global environment change. The helper only
    // acknowledges private children; it never executes sudo/sysctl or D-Bus.
    let child = CommandChild::spawn(
        command,
        &[("WIFI_CHILD_NONCE".into(), nonce.clone().into())],
    )
    .unwrap();
    let leader = Identity::open(Pid::from_child(&child.0.process));
    // Publish only a closed, complete record: opening the final pathname before
    // printf writes it lets this observer read an empty acknowledgement.
    wait_for(|| ready.is_file(), "owned helper acknowledgement");
    let record = fs::read_to_string(&ready).unwrap();
    let fields = record.split_whitespace().collect::<Vec<_>>();
    assert_eq!(fields.len(), 3);
    assert_eq!(fields[0].parse::<u32>().unwrap(), child.0.process.id());
    assert_eq!(fields[2], nonce);
    let sleeper_pid = Pid::from_raw(fields[1].parse().unwrap()).unwrap();
    let expected = format!("WIFI_CHILD_NONCE={nonce}");
    let same_owner = || {
        fs::read(format!("/proc/{}/environ", sleeper_pid.as_raw_pid())).is_ok_and(|env| {
            env.split(|byte| *byte == 0)
                .any(|entry| entry == expected.as_bytes())
        })
    };
    wait_for(same_owner, "acknowledged descendant carries private nonce");
    let sleeper = Identity::open(sleeper_pid);
    assert!(same_owner(), "identity recheck after pidfd admission");
    assert!(!leader.exited());
    assert!(!sleeper.exited());
    release(&control, mode);
    if leader_exits {
        wait_for(|| leader.exited(), "leader exits before cleanup");
        assert!(
            !sleeper.exited(),
            "actual acknowledged descendant survives leader"
        );
    }
    if mode == "complete" {
        wait_for(|| child.exited().unwrap(), "normal helper completion");
        assert!(
            sleeper.exited(),
            "helper normally reaped its own descendant"
        );
        assert!(child.exited().unwrap(), "completion polling retains leader");
        let observed = rustix::process::waitid(
            rustix::process::WaitId::Pid(Pid::from_child(&child.0.process)),
            rustix::process::WaitIdOptions::EXITED
                | rustix::process::WaitIdOptions::NOHANG
                | rustix::process::WaitIdOptions::NOWAIT,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            observed.exit_status(),
            Some(17),
            "retain actual nonzero completion before final reap"
        );
    }
    drop(child);
    wait_for(
        || leader.exited(),
        "actual helper leader must be terminated",
    );
    let until = Instant::now() + Duration::from_secs(1);
    while !sleeper.exited() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(sleeper.exited(), "production CommandChild cleanup left an acknowledged private descendant alive (leader_exits={leader_exits})");
}

#[test]
fn canceled_running_helper_cleans_its_acknowledged_descendant() {
    scenario("wait");
}
#[test]
fn completed_helper_cleans_its_acknowledged_surviving_descendant() {
    scenario("exit");
}

#[test]
fn normally_completed_helper_keeps_status_until_owned_cleanup() {
    scenario("complete");
}
