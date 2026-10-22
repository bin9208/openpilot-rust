use super::Supervisor;
use std::{
    cell::RefCell,
    io::{self, BufReader},
    os::unix::net::UnixListener,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn issue270_existing_executables_route_before_runtime_initialization() {
    let directory = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let before = rustix::process::child_subreaper().unwrap();
    for name in ["openpilot-carrot-server", "openpilot-carrot-command"] {
        let executable = directory.join(name);
        let temporary = tempfile::tempdir().unwrap();
        let socket = temporary.path().join("control");
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        let pair = openpilot_process_supervision::pty::Pair::open(30, 100).unwrap();
        let mut child = Command::new(&executable)
            .arg("--terminal-supervisor")
            .arg(directory.join("openpilot-process-child"))
            .arg(&socket)
            .args(["/bin/sh", "exec /bin/sleep 600"])
            // Managed-entry must never run in this mode, even when inherited
            // manager configuration is unusable for another server instance.
            .env("MANAGER_DAEMON", "invalid-supervisor-managed-name")
            .env(
                "OPENPILOT_BASEDIR",
                temporary.path().join("absent-repository"),
            )
            .stdin(Stdio::from(pair.slave.try_clone().unwrap()))
            .stdout(Stdio::from(pair.slave.try_clone().unwrap()))
            .stderr(Stdio::from(pair.slave))
            .spawn()
            .unwrap();
        let helper_pid = child.id();
        let deadline = Instant::now() + Duration::from_secs(5);
        let stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if let Some(status) = child.try_wait().unwrap() {
                        panic!("{name} did not route supervisor mode: {status}");
                    }
                    if Instant::now() >= deadline {
                        drop(listener);
                        child.kill().unwrap();
                        child.wait().unwrap();
                        panic!("{name} supervisor handshake timed out");
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(error) => {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("supervisor control accept: {error}");
                }
            }
        };
        let supervisor = Supervisor {
            _child: super::Helper(Some(child)),
            control: RefCell::new(BufReader::new(stream)),
            _directory: temporary,
        };
        supervisor
            .control
            .borrow()
            .get_ref()
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let greeting = supervisor.reply().unwrap();
        let leader: u32 = greeting.strip_prefix("PID ").unwrap().parse().unwrap();
        assert_ne!(helper_pid, leader);
        assert_eq!(
            std::fs::read_link(format!("/proc/{helper_pid}/exe")).unwrap(),
            executable.canonicalize().unwrap()
        );
        let stat = std::fs::read_to_string(format!("/proc/{leader}/stat")).unwrap();
        let fields: Vec<_> = stat
            .rsplit_once(')')
            .unwrap()
            .1
            .split_whitespace()
            .collect();
        assert_eq!(fields[1].parse::<u32>().unwrap(), helper_pid);
        assert_eq!(fields[3].parse::<u32>().unwrap(), leader);
        assert_eq!(supervisor.observe().unwrap(), None);
        drop(supervisor);
        assert!(!std::path::Path::new(&format!("/proc/{helper_pid}")).exists());
        assert!(!std::path::Path::new(&format!("/proc/{leader}")).exists());
        assert_eq!(rustix::process::child_subreaper().unwrap(), before);
    }
}

#[test]
fn issue270_stopped_prehandshake_helper_is_resumed_and_reaped() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let directory = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let temporary = tempfile::tempdir().unwrap();
    let wrapper = temporary.path().join("openpilot-carrot-server");
    let server = directory.join("openpilot-carrot-server");
    let script = format!(
        "#!/bin/sh\nkill -STOP $$\nexec '{}' \"$@\"\n",
        server.display()
    );
    std::fs::write(&wrapper, script).unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
    symlink(
        directory.join("openpilot-process-child"),
        temporary.path().join("openpilot-process-child"),
    )
    .unwrap();
    let config = config_for_supervisor_test(temporary.path());
    let pair = openpilot_process_supervision::pty::Pair::open(30, 100).unwrap();
    let began = Instant::now();
    let (supervisor, leader) = Supervisor::start_until(
        &config,
        pair.slave,
        "exec sleep 600",
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    drop(supervisor);
    assert!(began.elapsed() < Duration::from_secs(6));
    assert!(!std::path::Path::new(&format!("/proc/{}", leader.as_raw_nonzero())).exists());
}

#[test]
fn issue270_prehandshake_timeout_escalates_and_reaps() {
    use std::os::unix::fs::PermissionsExt;
    let temporary = tempfile::tempdir().unwrap();
    let wrapper = temporary.path().join("openpilot-carrot-server");
    let pid_file = temporary.path().join("pid");
    std::fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\nprintf '%s' $$ > '{}'\nexec /bin/sleep 600\n",
            pid_file.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
    let config = config_for_supervisor_test(temporary.path());
    let pair = openpilot_process_supervision::pty::Pair::open(30, 100).unwrap();
    let began = Instant::now();
    assert!(Supervisor::start_until(
        &config,
        pair.slave,
        "unused",
        Instant::now() + Duration::from_millis(100)
    )
    .is_err());
    assert!(began.elapsed() < Duration::from_secs(6));
    let pid: u32 = std::fs::read_to_string(pid_file).unwrap().parse().unwrap();
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
}

fn config_for_supervisor_test(directory: &std::path::Path) -> super::Config {
    super::Config {
        launcher: directory.join("openpilot-process-child"),
        cli: directory.join("openpilot-carrot-command"),
        cli_cwd: directory.into(),
        start_dir: directory.into(),
        motd_dir: directory.into(),
        motd_cache: directory.join("unused"),
        tmux_log: directory.join("unused"),
        web_session: "unused-270".into(),
        capture_lines: 160,
    }
}
