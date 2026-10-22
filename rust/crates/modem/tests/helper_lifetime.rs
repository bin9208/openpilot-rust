//! Private Linux subprocesses only: never invoke host sudo/ip/pppd/systemctl.
#![cfg(target_os = "linux")]
use openpilot_modem::{config::Config, ppp, Error};
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
};
use std::{
    fs,
    os::{fd::OwnedFd, unix::fs::symlink},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

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
    fn signal(&self, signal: Signal) {
        pidfd_send_signal(&self.0, signal).unwrap();
    }
}
impl Drop for Identity {
    fn drop(&mut self) {
        // Fallback runs only after the real survival assertions. No numeric-PID kill.
        let _ = pidfd_send_signal(&self.0, Signal::KILL);
    }
}
struct NonceCleanup(String);
impl Drop for NonceCleanup {
    fn drop(&mut self) {
        let Ok(entries) = fs::read_dir("/proc") else {
            return;
        };
        for entry in entries.flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|v| v.parse().ok())
                .and_then(Pid::from_raw)
            else {
                continue;
            };
            if !has_nonce(pid, &self.0) {
                continue;
            }
            let Ok(fd) = pidfd_open(pid, PidfdFlags::empty()) else {
                continue;
            };
            if has_nonce(pid, &self.0) {
                let _ = pidfd_send_signal(&fd, Signal::KILL);
            }
        }
    }
}
fn has_nonce(pid: Pid, nonce: &str) -> bool {
    let expected = format!("MODEM_HELPER_NONCE={nonce}");
    fs::read(format!("/proc/{}/environ", pid.as_raw_pid()))
        .is_ok_and(|env| env.split(|ch| *ch == 0).any(|v| v == expected.as_bytes()))
}
fn wait_for(mut predicate: impl FnMut() -> bool, timeout: Duration, description: &str) {
    let until = Instant::now() + timeout;
    while !predicate() {
        assert!(Instant::now() < until, "{description}");
        thread::sleep(Duration::from_millis(5));
    }
}
fn quote(path: &Path) -> String {
    format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"))
}
struct Fixture {
    _cleanup: NonceCleanup,
    root: tempfile::TempDir,
    config: Config,
    nonce: String,
}
impl Fixture {
    fn new(mode: &str) -> Self {
        let root = tempfile::Builder::new()
            .prefix("modem-helper-owner-")
            .tempdir()
            .unwrap();
        let nonce = root
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let payload = root.path().join("payload");
        // Initial env is established by exec, so /proc/environ can validate identities.
        let script = format!(
            r#"#!/bin/sh
set -eu
cd {root}
printf '%s\n' "$*" >> calls
case "$*" in
  'ip rule del table 1000') exit 1 ;;
esac
case {mode} in
  ppp) [ "$1" = pppd ] || exit 0 ;;
  mask) [ "$*" = 'systemctl mask --runtime ModemManager' ] || exit 0 ;;
  stop) [ "$*" = 'systemctl stop ModemManager' ] || exit 0 ;;
  unmask) [ "$*" = 'systemctl unmask --runtime ModemManager' ] || exit 0 ;;
  stdin) IFS= read -r value; [ "$value" = modem-interface-stdin ]; printf '  inet 10.0.0.2 peer 10.0.0.1/32 scope global ppp0\n'; exit 0 ;;
  success) printf '  inet 10.0.0.2 peer 10.0.0.1/32 scope global ppp0\n'; exit 0 ;;
  nonzero) printf '  inet 10.0.0.2 peer 10.0.0.1/32 scope global ppp0\n'; exit 7 ;;
  invalid) printf '\377'; exit 0 ;;
  invaliderr) printf '  inet 10.0.0.2 peer 10.0.0.1/32 scope global ppp0\n'; printf '\377' >&2; exit 0 ;;
esac
case {mode} in
  stderr) /bin/sleep 60 > /dev/null & ;;
  finite) /bin/sh -c 'while [ ! -f finish ]; do /bin/sleep 0.01; done' & ;;
  *) /bin/sleep 60 & ;;
esac
sleeper=$!
printf '%s %s %s\n' "$$" "$sleeper" "$MODEM_HELPER_NONCE" > ready.new
mv ready.new ready
while [ ! -f release ]; do /bin/sleep 0.01; done
case {mode} in
  stdout|stderr|finite)
    printf '  inet 10.0.0.2 peer 10.0.0.1/32 scope global ppp0\n'
    exit 0 ;;
  complete) kill "$sleeper"; wait "$sleeper" || :; exit 17 ;;
  orphan) exit 0 ;;
  *) wait "$sleeper" ;;
esac
"#,
            root = quote(root.path()),
            mode = quote(Path::new(mode))
        );
        fs::write(&payload, script).unwrap();
        let executable = root.path().join("private-command");
        fs::write(root.path().join("nonce"), format!("{nonce}\n")).unwrap();
        // Kernel exec targets a tracked immutable file. Only the read-only shell
        // payload is generated at runtime, so concurrent forks cannot retain a
        // write descriptor to an executable and make another test hit ETXTBSY.
        symlink(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/helper-entry.sh"),
            &executable,
        )
        .unwrap();
        let launcher = PathBuf::from(
            std::env::var_os("MODEM_PROCESS_CHILD_BINARY")
                .expect("MODEM_PROCESS_CHILD_BINARY must name fresh openpilot-process-child"),
        );
        assert!(launcher.is_absolute() && launcher.is_file());
        for directory in ["params", "statistics"] {
            fs::create_dir(root.path().join(directory)).unwrap();
        }
        let config = Config {
            at_port: root.path().join("absent-at"),
            ppp_port: root.path().join("absent-ppp"),
            lock: root.path().join("lock"),
            state: root.path().join("state"),
            params: root.path().join("params"),
            statistics: root.path().join("statistics"),
            modem_manager: root.path().join("manager.service"),
            sudo: executable.clone(),
            ip: executable,
            launcher,
            state_wait_ms: 60_000,
            ..Config::default()
        };
        Self {
            _cleanup: NonceCleanup(nonce.clone()),
            root,
            config,
            nonce,
        }
    }
    fn acknowledged(&self) -> (Identity, Identity) {
        let ready = self.root.path().join("ready");
        wait_for(
            || ready.is_file(),
            Duration::from_secs(5),
            "helper acknowledgement",
        );
        let record = fs::read_to_string(ready).unwrap();
        assert!(record.ends_with('\n'), "complete acknowledgement");
        let fields = record.split_whitespace().collect::<Vec<_>>();
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[2], self.nonce);
        let admit = |value: &str| {
            let pid = Pid::from_raw(value.parse().unwrap()).unwrap();
            assert!(has_nonce(pid, &self.nonce));
            let identity = Identity::open(pid);
            assert!(
                has_nonce(pid, &self.nonce),
                "identity recheck after pidfd_open"
            );
            assert!(!identity.exited());
            identity
        };
        (admit(fields[0]), admit(fields[1]))
    }
    fn release(&self) {
        fs::write(self.root.path().join("release"), b"release\n").unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(root) = std::env::var_os("MODEM_HELPER_EVIDENCE") {
            let target = PathBuf::from(root).join(&self.nonce);
            if let Err(error) = fs::create_dir_all(&target) {
                eprintln!("modem fixture evidence: {error}");
                return;
            }
            for name in [
                "calls",
                "ready",
                "daemon.stderr",
                "source.stdout",
                "source.stderr",
                "config.json",
            ] {
                let from = self.root.path().join(name);
                if from.is_file() {
                    if let Err(error) = fs::copy(from, target.join(name)) {
                        eprintln!("modem fixture evidence {name}: {error}");
                    }
                }
            }
        }
    }
}

fn assert_gone(identity: &Identity, description: &str) {
    wait_for(|| identity.exited(), Duration::from_secs(1), description);
}
fn interface_timeout(mode: &str) {
    let fixture = Fixture::new(mode);
    let config = fixture.config.clone();
    let started = Instant::now();
    let task = thread::spawn(move || ppp::interface(&config));
    let (leader, descendant) = fixture.acknowledged();
    fixture.release();
    if matches!(mode, "stdout" | "stderr") {
        wait_for(
            || leader.exited(),
            Duration::from_secs(1),
            "leader exit before pipe EOF",
        );
        assert!(!descendant.exited(), "live writer admitted before result");
    }
    let result = task.join().unwrap();
    assert!(
        matches!(result, Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::TimedOut),
        "must time out, not accept leader's valid inet line"
    );
    assert!(started.elapsed() >= Duration::from_millis(1900));
    assert!(started.elapsed() < Duration::from_secs(4));
    assert_gone(&leader, "helper leader cleanup");
    assert_gone(&descendant, "helper descendant cleanup before fallback");
}
#[test]
fn interface_timeout_cleans_live_helper_group() {
    interface_timeout("live");
}
#[test]
fn interface_deadline_includes_stdout_eof_after_leader_exit() {
    interface_timeout("stdout");
}
#[test]
fn interface_deadline_includes_stderr_eof_after_leader_exit() {
    interface_timeout("stderr");
}
#[test]
fn interface_accepts_finite_pipe_eof_after_leader_exit() {
    let fixture = Fixture::new("finite");
    let config = fixture.config.clone();
    let task = thread::spawn(move || ppp::interface(&config));
    let (leader, descendant) = fixture.acknowledged();
    fixture.release();
    wait_for(
        || leader.exited(),
        Duration::from_secs(1),
        "leader exits before finite EOF",
    );
    assert!(!descendant.exited());
    assert!(
        !task.is_finished(),
        "leader exit alone is not capture completion"
    );
    fs::write(fixture.root.path().join("finish"), b"close pipes\n").unwrap();
    assert_eq!(
        task.join().unwrap().unwrap(),
        ("10.0.0.2".into(), "10.0.0.1".into())
    );
    assert_gone(&descendant, "finite writer completes normally");
}

#[test]
fn interface_retains_original_nonzero_status_and_utf8_behavior() {
    for mode in ["success", "nonzero"] {
        let fixture = Fixture::new(mode);
        assert_eq!(
            ppp::interface(&fixture.config).unwrap(),
            ("10.0.0.2".into(), "10.0.0.1".into())
        );
    }
    for mode in ["invalid", "invaliderr"] {
        let fixture = Fixture::new(mode);
        assert!(
            matches!(ppp::interface(&fixture.config), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData)
        );
    }
}
#[test]
fn normal_command_exit_cleans_descendant_without_reaping_leader_first() {
    let fixture = Fixture::new("orphan");
    let config = fixture.config.clone();
    let task = thread::spawn(move || ppp::run(&config, &["owned-control"]));
    let (leader, descendant) = fixture.acknowledged();
    fixture.release();
    assert!(task.join().unwrap().unwrap());
    assert_gone(&leader, "command leader exit");
    assert_gone(&descendant, "normal command orphan cleanup before fallback");
}
#[test]
fn ordinary_commands_keep_exit_status_and_no_two_second_deadline() {
    let fixture = Fixture::new("complete");
    let config = fixture.config.clone();
    let task = thread::spawn(move || ppp::run(&config, &["owned-control"]));
    let (leader, descendant) = fixture.acknowledged();
    thread::sleep(Duration::from_millis(2200));
    assert!(
        !task.is_finished(),
        "interface deadline must not apply to other commands"
    );
    assert!(!leader.exited() && !descendant.exited());
    fixture.release();
    assert!(
        !task.join().unwrap().unwrap(),
        "actual exit17 remains false"
    );
    assert_gone(&leader, "normal leader completion");
    assert_gone(&descendant, "normal descendant completion");
}
#[test]
fn dropping_ppp_session_cleans_live_owned_children() {
    let fixture = Fixture::new("live");
    let mut session = ppp::Session::default();
    session.start(&fixture.config).unwrap();
    let (leader, descendant) = fixture.acknowledged();
    fixture.release();
    drop(session);
    assert_gone(&leader, "PPP leader dropped");
    assert_gone(&descendant, "PPP descendant dropped before fallback");
}
#[test]
fn exited_ppp_leader_is_retained_until_session_cleanup() {
    let fixture = Fixture::new("orphan");
    let mut session = ppp::Session::default();
    session.start(&fixture.config).unwrap();
    let (leader, descendant) = fixture.acknowledged();
    fixture.release();
    wait_for(
        || session.has_exited().unwrap(),
        Duration::from_secs(1),
        "PPP exit observation",
    );
    assert!(leader.exited());
    assert!(
        !descendant.exited(),
        "descendant alive before session cleanup"
    );
    drop(session);
    assert_gone(&descendant, "PPP orphan cleanup after exit observation");
}

#[test]
fn killed_ppp_remains_observably_exited_until_a_new_session_starts() {
    let fixture = Fixture::new("ppp");
    let mut session = ppp::Session::default();
    assert!(
        !session.has_exited().unwrap(),
        "never started is not an exited PPP"
    );
    session.kill(&fixture.config).unwrap();
    assert!(
        !session.has_exited().unwrap(),
        "cleanup without PPP preserves never-started state"
    );
    session.start(&fixture.config).unwrap();
    let (leader, descendant) = fixture.acknowledged();
    fixture.release();
    assert!(!session.has_exited().unwrap());
    session.kill(&fixture.config).unwrap();
    assert!(
        session.has_exited().unwrap(),
        "route-failure cleanup must remain visible to WaitExit/reconnect"
    );
    assert_gone(&leader, "killed PPP leader");
    assert_gone(&descendant, "killed PPP descendant before fallback");
    // Repeated stop/observation uses cached completion, never signals a reaped
    // leader's numeric process-group identity again.
    session.kill(&fixture.config).unwrap();
    assert!(session.has_exited().unwrap());
    fs::remove_file(fixture.root.path().join("ready")).unwrap();
    fs::remove_file(fixture.root.path().join("release")).unwrap();
    session.start(&fixture.config).unwrap();
    let (next_leader, next_descendant) = fixture.acknowledged();
    fixture.release();
    assert!(
        !session.has_exited().unwrap(),
        "new child resets old completion"
    );
    session.kill(&fixture.config).unwrap();
    assert!(session.has_exited().unwrap());
    assert_gone(&next_leader, "new PPP leader cleanup");
    assert_gone(&next_descendant, "new PPP descendant cleanup");
}

struct Daemon(Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn cancelled_startup(mode: &str, signal: Signal) {
    let fixture = Fixture::new(mode);
    fs::write(&fixture.config.modem_manager, b"private marker").unwrap();
    let config = fixture.root.path().join("config.json");
    fs::write(&config, serde_json::to_vec(&fixture.config).unwrap()).unwrap();
    let log = fs::File::create(fixture.root.path().join("daemon.stderr")).unwrap();
    let mut daemon = Daemon(
        Command::new(env!("CARGO_BIN_EXE_openpilot-modem"))
            .args(["--config"])
            .arg(config)
            .env_remove("MANAGER_DAEMON")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    let daemon_id = Identity::open(Pid::from_child(&daemon.0));
    let (leader, descendant) = fixture.acknowledged();
    fixture.release();
    let started = Instant::now();
    // Leader only: group broadcasting here would mask the missing cancellation path.
    daemon_id.signal(signal);
    wait_for(
        || daemon_id.exited(),
        Duration::from_secs(2),
        "daemon observes signal during systemctl wait",
    );
    assert!(daemon.0.wait().unwrap().success());
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_gone(&leader, "cancelled startup helper");
    assert_gone(&descendant, "cancelled startup descendant before fallback");
    assert!(!fixture.config.state.exists(), "normal stop removes state");
    let calls = fs::read_to_string(fixture.root.path().join("calls")).unwrap();
    assert!(calls.contains("systemctl unmask --runtime ModemManager\n"));
    assert!(calls.contains("systemctl start ModemManager\n"));
}
#[test]
fn sigint_interrupts_owned_mask_helper_without_group_broadcast() {
    cancelled_startup("mask", Signal::INT);
}
#[test]
fn sigterm_interrupts_owned_stop_helper_without_group_broadcast() {
    cancelled_startup("stop", Signal::TERM);
}

#[test]
fn subsequent_signal_interrupts_shutdown_helper_without_skipping_normal_restoration() {
    let fixture = Fixture::new("unmask");
    fs::write(&fixture.config.modem_manager, b"private marker").unwrap();
    let config = fixture.root.path().join("config.json");
    fs::write(&config, serde_json::to_vec(&fixture.config).unwrap()).unwrap();
    let log = fs::File::create(fixture.root.path().join("daemon.stderr")).unwrap();
    let mut daemon = Daemon(
        Command::new(env!("CARGO_BIN_EXE_openpilot-modem"))
            .arg("--config")
            .arg(config)
            .env_remove("MANAGER_DAEMON")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    let daemon_id = Identity::open(Pid::from_child(&daemon.0));
    wait_for(
        || {
            fs::read_to_string(fixture.root.path().join("calls"))
                .is_ok_and(|calls| calls.contains("killall -9 pppd\n"))
        },
        Duration::from_secs(5),
        "runtime startup commands",
    );
    daemon_id.signal(Signal::INT);
    let (leader, descendant) = fixture.acknowledged();
    fixture.release();
    assert!(
        !daemon_id.exited(),
        "normal restoration is not skipped or given a new timeout"
    );
    daemon_id.signal(Signal::TERM);
    wait_for(
        || daemon_id.exited(),
        Duration::from_secs(2),
        "signal during restoration helper",
    );
    assert!(daemon.0.wait().unwrap().success());
    assert_gone(&leader, "shutdown helper cleanup");
    assert_gone(&descendant, "shutdown descendant cleanup before fallback");
    assert!(!fixture.config.state.exists());
}

#[test]
fn helper_inherits_stdin_while_outputs_are_captured() {
    if let Some(path) = std::env::var_os("MODEM_STDIN_PROBE_CONFIG") {
        let config: Config = serde_json::from_reader(fs::File::open(path).unwrap()).unwrap();
        assert_eq!(
            ppp::interface(&config).unwrap(),
            ("10.0.0.2".into(), "10.0.0.1".into())
        );
        return;
    }
    let fixture = Fixture::new("stdin");
    let config_path = fixture.root.path().join("config.json");
    fs::write(&config_path, serde_json::to_vec(&fixture.config).unwrap()).unwrap();
    let input = fixture.root.path().join("stdin");
    fs::write(&input, b"modem-interface-stdin\n").unwrap();
    let output_path = fixture.root.path().join("source.stdout");
    let log_path = fixture.root.path().join("source.stderr");
    let mut probe = Daemon(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "helper_inherits_stdin_while_outputs_are_captured",
                "--nocapture",
            ])
            .env("MODEM_STDIN_PROBE_CONFIG", config_path)
            .stdin(fs::File::open(input).unwrap())
            .stdout(fs::File::create(output_path).unwrap())
            .stderr(fs::File::create(&log_path).unwrap())
            .spawn()
            .unwrap(),
    );
    let identity = Identity::open(Pid::from_child(&probe.0));
    wait_for(
        || identity.exited(),
        Duration::from_secs(5),
        "owned inherited-stdin probe",
    );
    assert!(
        probe.0.wait().unwrap().success(),
        "{}",
        fs::read_to_string(log_path).unwrap()
    );
}

#[test]
fn unchanged_original_waits_for_retained_stdout_and_stderr() {
    // This invokes the retained original in CI, not a rewritten source policy.
    let python = std::env::var_os("MODEM_SOURCE_PYTHON").expect("CI source Python prerequisite");
    let tools = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools");
    for mode in ["stdout", "stderr", "invaliderr"] {
        let fixture = Fixture::new(mode);
        let config_path = fixture.root.path().join("config.json");
        fs::write(&config_path, serde_json::to_vec(&fixture.config).unwrap()).unwrap();
        let output = fs::File::create(fixture.root.path().join("source.stdout")).unwrap();
        let log = fs::File::create(fixture.root.path().join("source.stderr")).unwrap();
        let mut source = Daemon(Command::new(&python).arg("-c").arg(
            "import json,sys; sys.path.insert(0,sys.argv[1]); from modem_source import load; m,modem=load(json.load(open(sys.argv[2]))); print(json.dumps(modem._poll_iface()))"
        ).arg(&tools).arg(config_path).stdout(output).stderr(log).stdin(Stdio::null()).spawn().unwrap());
        let source_id = Identity::open(Pid::from_child(&source.0));
        let identities = if mode == "invaliderr" {
            None
        } else {
            let identities = fixture.acknowledged();
            fixture.release();
            Some(identities)
        };
        wait_for(
            || source_id.exited(),
            Duration::from_secs(5),
            "original bounded interface request",
        );
        assert!(source.0.wait().unwrap().success());
        assert_eq!(
            fs::read_to_string(fixture.root.path().join("source.stdout"))
                .unwrap()
                .trim(),
            "{}"
        );
        if let Some((leader, descendant)) = identities {
            assert_gone(&leader, "original direct-child reap");
            // Original direct-child timeout leaves this child; never relabel that inherited
            // behavior as native parity. The identity guard cleans it AFTER observation.
            assert!(
                !descendant.exited(),
                "retained original does not own descendants"
            );
        }
        if let Some(capture) = std::env::var_os("MODEM_SOURCE_REFERENCE_CAPTURE") {
            let directory = PathBuf::from(capture).join(mode);
            fs::create_dir(&directory).expect("new original-reference capture directory");
            let stdout = fs::read(fixture.root.path().join("source.stdout")).unwrap();
            let stderr = fs::read(fixture.root.path().join("source.stderr")).unwrap();
            fs::write(directory.join("stdout"), &stdout).unwrap();
            fs::write(directory.join("stderr"), &stderr).unwrap();
            let record = serde_json::json!({
                "mode":mode,"original_exit_code":0,"stdout_bytes":stdout,
                "stderr_bytes":stderr,"direct_child_reaped":true,
                "descendant_alive_after_direct_child_exit":mode != "invaliderr",
                "invalid_stderr_was_rejected":mode == "invaliderr",
                "original_result":{},"original_only":true
            });
            fs::write(
                directory.join("observation.json"),
                serde_json::to_vec_pretty(&record).unwrap(),
            )
            .unwrap();
        }
    }
}
