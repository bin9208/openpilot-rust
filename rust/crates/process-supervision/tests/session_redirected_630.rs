//! #630: actual launcher/session/stdio boundaries, using only private subprocesses.
#![cfg(target_os = "linux")]
use openpilot_process_supervision::{CapturedChild, CapturedCommand};
use rustix::process::{
    getpgid, getpgrp, getsid, kill_process_group, waitid, Pid, Signal, WaitId, WaitIdOptions,
};
use std::{
    fs::{self, File},
    io::Read,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Probe(Child);
impl Drop for Probe {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Session {
    child: CapturedChild,
    cleanup_started: bool,
}
impl Session {
    fn cleanup(&mut self) {
        if !self.cleanup_started {
            self.cleanup_started = true;
            let _ = kill_process_group(Pid::from_child(&self.child.process), Signal::KILL);
            let _ = self.child.process.wait();
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.cleanup();
    }
}
fn until(mut predicate: impl FnMut() -> bool, description: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !predicate() {
        assert!(Instant::now() < deadline, "{description}");
        thread::sleep(Duration::from_millis(5));
    }
}

fn exercise(mode: &str, name: &str) {
    if let Some(root) = std::env::var_os("SUPPORT630_SESSION_PROBE_ROOT") {
        probe(Path::new(&root), mode);
        return;
    }
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("input"), b"private-stdin-630\n").unwrap();
    let out = root.path().join("probe.stdout");
    let err = root.path().join("probe.stderr");
    let mut child = Probe(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture"])
            .env("SUPPORT630_SESSION_PROBE_ROOT", root.path())
            .env_remove("SUPPORT630_VALUE")
            .stdin(File::open(root.path().join("input")).unwrap())
            .stdout(File::create(&out).unwrap())
            .stderr(File::create(&err).unwrap())
            .spawn()
            .unwrap(),
    );
    let mut status = None;
    until(
        || {
            status = child.0.try_wait().unwrap();
            status.is_some()
        },
        "private stdio probe completion",
    );
    assert!(
        status.unwrap().success(),
        "stdout={} stderr={}",
        fs::read_to_string(&out).unwrap(),
        fs::read_to_string(&err).unwrap()
    );
    if mode == "inherited" {
        let stdout = fs::read_to_string(out).unwrap();
        let stderr = fs::read_to_string(err).unwrap();
        // libtest may print its progress prefix on stdout without a newline.
        assert!(stdout.contains("out:private-env-630:private-stdin-630\n"));
        assert!(stderr.contains("err:private-env-630:private-stdin-630\n"));
        assert!(!stdout.contains("err:private-env-630:"));
        assert!(!stderr.contains("out:private-env-630:"));
    }
}

fn probe(root: &Path, mode: &str) {
    let parent_group = getpgrp();
    let parent_session = getsid(None).unwrap();
    let command = CapturedCommand {
        launcher: env!("CARGO_BIN_EXE_openpilot-process-child").into(),
        cwd: root.into(),
        argv: [
            "/bin/sh",
            "-c",
            "value=EOF; IFS= read -r value || value=EOF; printf 'out:%s:%s\n' \"$SUPPORT630_VALUE\" \"$value\"; printf 'err:%s:%s\n' \"$SUPPORT630_VALUE\" \"$value\" >&2; printf ready > ready; n=0; while [ ! -f release ] && [ \"$n\" -lt 500 ]; do /bin/sleep 0.01; n=$((n+1)); done; [ -f release ] || exit 99; exit 17",
        ]
        .into_iter()
        .map(Into::into)
        .collect(),
    };
    let environment = [("SUPPORT630_VALUE".into(), "private-env-630".into())];
    let child = match mode {
        "redirected" => {
            command.spawn_session_redirected_with_env(Stdio::piped(), Stdio::piped(), &environment)
        }
        "inherited" => command.spawn_session_inherited_with_env(&environment),
        // Existing API control: stdin must remain EOF, even with live inherited input.
        "legacy" => command.spawn_session_redirected(Stdio::piped(), Stdio::piped()),
        _ => unreachable!(),
    }
    .unwrap();
    let mut session = Session {
        child,
        cleanup_started: false,
    };
    until(
        || root.join("ready").is_file(),
        "helper acknowledges stdin and outputs",
    );
    let pid = Pid::from_child(&session.child.process);
    assert_eq!(getpgid(Some(pid)).unwrap(), pid);
    assert_eq!(getsid(Some(pid)).unwrap(), pid);
    assert_ne!(pid, parent_group);
    assert_ne!(pid, parent_session);
    assert_eq!(getpgrp(), parent_group);
    assert_eq!(getsid(None).unwrap(), parent_session);
    assert!(waitid(
        WaitId::Pid(pid),
        WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
    )
    .unwrap()
    .is_none());
    if mode == "inherited" {
        assert!(session.child.process.stdout.is_none());
        assert!(session.child.process.stderr.is_none());
    }
    fs::write(root.join("release"), b"release\n").unwrap();
    until(
        || {
            waitid(
                WaitId::Pid(pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            )
            .unwrap()
            .is_some()
        },
        "retained session leader completion",
    );
    // The API leaves ownership to its caller: signal the retained group before reap.
    match kill_process_group(pid, Signal::KILL) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => {}
        Err(error) => panic!("owned session cleanup: {error}"),
    }
    session.cleanup_started = true;
    assert_eq!(session.child.process.wait().unwrap().code(), Some(17));
    if mode != "inherited" {
        let mut stdout = String::new();
        let mut stderr = String::new();
        session
            .child
            .process
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut stdout)
            .unwrap();
        session
            .child
            .process
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut stderr)
            .unwrap();
        let (value, input) = if mode == "legacy" {
            ("", "EOF")
        } else {
            ("private-env-630", "private-stdin-630")
        };
        assert_eq!(stdout, format!("out:{value}:{input}\n"));
        assert_eq!(stderr, format!("err:{value}:{input}\n"));
    }
}

#[test]
fn redirected_session_inherits_stdin_and_separates_outputs() {
    exercise(
        "redirected",
        "redirected_session_inherits_stdin_and_separates_outputs",
    );
}

#[test]
fn inherited_session_preserves_stdin_stdout_stderr_and_environment() {
    exercise(
        "inherited",
        "inherited_session_preserves_stdin_stdout_stderr_and_environment",
    );
}

#[test]
fn existing_session_redirected_keeps_null_stdin() {
    exercise("legacy", "existing_session_redirected_keeps_null_stdin");
}
