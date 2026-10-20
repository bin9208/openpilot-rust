use openpilot_dev_tools::paramsd_startup_gate::StartupGate;
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    io::{read, write},
    process::{kill_process_group, Pid, Signal},
};
use std::{
    fs,
    os::{fd::AsRawFd, unix::process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[test]
fn independent_original_sources_remain_unchanged() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for (name, expected) in [
        (
            "rust/tools/paramsd_startup_gate.py",
            "d1fb549b84b3182639191a86988516baf10a8fc1a28758e046e25077d64f03f4",
        ),
        (
            "rust/tools/tests/test_paramsd_atomic_gate.py",
            "2c289464407f46382af24d01ab9459ef3e72178d0b00caac1ef8e295437701d9",
        ),
    ] {
        assert_eq!(
            openpilot_dev_tools::seal::digest(&root.join(name)).unwrap(),
            expected
        );
    }
}

fn gate_for_cursor(root: &Path, cursor: u64) -> StartupGate {
    let queue = root.join("livePose");
    let mut bytes = vec![0; 256_984];
    bytes[..8].copy_from_slice(&1_u64.to_le_bytes());
    bytes[8..16].copy_from_slice(&cursor.to_le_bytes());
    fs::write(&queue, bytes).unwrap();
    StartupGate::new(&queue, root).unwrap()
}

fn release_ready(gate: &StartupGate) -> bool {
    let descriptor = gate.release_reader();
    poll(
        &mut [PollFd::new(&descriptor, PollFlags::IN)],
        Some(&Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        }),
    )
    .unwrap()
        != 0
}

#[test]
fn complete_pose_commit_releases_normal_cursor() {
    let root = tempfile::tempdir().unwrap();
    let mut gate = gate_for_cursor(root.path(), 232);
    gate.release(0, 224).unwrap();
    assert!(release_ready(&gate));
    let mut token = [0];
    assert_eq!(read(gate.release_reader(), &mut token).unwrap(), 1);
    assert_eq!(token, *b"R");
    assert_eq!(gate.report().batches_queued_before_release, 1);
    assert!(gate.report().exact_committed_pose_cursor);
    assert!(root.path().join("startup-poll-gate.json").exists());
}

#[test]
fn complete_pose_commit_releases_wrapped_cursor() {
    let root = tempfile::tempdir().unwrap();
    let mut gate = gate_for_cursor(root.path(), 4_294_967_528);
    gate.release(255_896, 224).unwrap();
    assert!(release_ready(&gate));
    let mut token = [0];
    assert_eq!(read(gate.release_reader(), &mut token).unwrap(), 1);
    assert_eq!(token, *b"R");
}

#[test]
fn complete_pose_commit_releases_cycle_overflow_cursor() {
    let root = tempfile::tempdir().unwrap();
    let mut gate = gate_for_cursor(root.path(), 232);
    gate.release(0xffff_ffff_0003_e798, 224).unwrap();
    assert!(release_ready(&gate));
    let mut token = [0];
    assert_eq!(read(gate.release_reader(), &mut token).unwrap(), 1);
    assert_eq!(token, *b"R");
}

#[test]
fn wrap_header_alone_does_not_release_reader() {
    let root = tempfile::tempdir().unwrap();
    let mut gate = gate_for_cursor(root.path(), 4_294_967_296);
    assert!(gate.release(255_896, 224).is_err());
    assert!(!release_ready(&gate));
    assert_eq!(gate.report().batches_queued_before_release, 0);
    assert!(!root.path().join("startup-poll-gate.json").exists());
}

#[test]
fn unrelated_pose_size_does_not_release_reader() {
    let root = tempfile::tempdir().unwrap();
    let mut gate = gate_for_cursor(root.path(), 4_294_967_520);
    assert!(gate.release(255_896, 224).is_err());
    assert!(!release_ready(&gate));
    assert_eq!(gate.report().batches_queued_before_release, 0);
}

#[test]
fn ready_and_finish_tokens_use_owned_real_pipes() {
    let root = tempfile::tempdir().unwrap();
    let mut gate = gate_for_cursor(root.path(), 0);
    for descriptor in [gate.ready_writer(), gate.release_reader()] {
        assert!(rustix::io::fcntl_getfd(descriptor)
            .unwrap()
            .contains(rustix::io::FdFlags::CLOEXEC));
    }
    assert_eq!(write(gate.ready_writer(), b"R").unwrap(), 1);
    gate.wait_batch().unwrap();
    gate.finish().unwrap();
    let mut token = [0];
    assert_eq!(read(gate.release_reader(), &mut token).unwrap(), 1);
    assert_eq!(token, *b"S");
}

struct OwnedDaemon {
    child: Child,
    reaped: bool,
}
impl Drop for OwnedDaemon {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = kill_process_group(Pid::from_child(&self.child), Signal::KILL);
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
fn required_path(name: &str) -> PathBuf {
    let path = PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} required")));
    assert!(path.is_absolute(), "{name} must be absolute");
    path
}

/// Cloud-only actual consumer of the scoped child-FD handoff. The complete
/// 1,202-frame original/native daemon comparison remains a separate gate.
#[test]
#[ignore = "requires source-built phase hook, original CarParams, and fresh daemon ELF in Actions"]
fn native_mapped_gate_holds_actual_paramsd_second_poll_and_shutdown() {
    let daemon = required_path("PARAMSD_NATIVE_GATE_DAEMON");
    let library = required_path("PARAMSD_NATIVE_GATE_LIBRARY");
    let car = required_path("PARAMSD_NATIVE_GATE_CAR_PARAMS");
    let evidence = required_path("PARAMSD_NATIVE_GATE_EVIDENCE");
    fs::create_dir(&evidence).expect("native paramsd evidence must be fresh");
    assert!(daemon.is_file() && library.is_file() && car.is_file());
    assert_eq!(
        openpilot_dev_tools::seal::digest(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../..")
                .join("rust/tools/paramsd_startup_phase.c")
        )
        .unwrap(),
        "1b524e78cdfa1cb4013a35cb570fd2fbc03cce7a0153a5ef8ba1627102265a3b"
    );
    let private = tempfile::tempdir().unwrap();
    let shm = tempfile::Builder::new()
        .prefix("msgq_rust-probe-params-")
        .tempdir_in("/dev/shm")
        .unwrap();
    let prefix = shm
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .strip_prefix("msgq_")
        .unwrap();
    let params = private.path().join("params");
    let memory = private.path().join("memory");
    fs::create_dir_all(params.join(prefix)).unwrap();
    fs::create_dir_all(memory.join(prefix)).unwrap();
    fs::write(params.join(prefix).join("UbloxAvailable"), b"1").unwrap();
    fs::copy(&car, params.join(prefix).join("CarParams")).unwrap();
    let position = memory.join(prefix).join("LastGPSPosition");
    fs::write(&position, b"old position").unwrap();
    let trace = evidence.join("poll.jsonl");
    let mut gate = StartupGate::new(&shm.path().join("livePose"), &evidence).unwrap();
    let child_gate_fds = [
        gate.ready_writer().as_raw_fd(),
        gate.release_reader().as_raw_fd(),
    ];
    let mut command = Command::new(&daemon);
    command
        .arg("--memory-root")
        .arg(&memory)
        .env("OPENPILOT_PREFIX", prefix)
        .env("PARAMS_ROOT", &params)
        .env("DEBUG", "1")
        .env("REPLAY", "1")
        .env("SIMULATION", "1")
        .env_remove("ZMQ")
        .env_remove("CEREAL_FAKE")
        .stdin(Stdio::null())
        .stdout(Stdio::from(
            fs::File::create(evidence.join("stdout.log")).unwrap(),
        ))
        .stderr(Stdio::from(
            fs::File::create(evidence.join("stderr.log")).unwrap(),
        ))
        .process_group(0);
    gate.attach(&mut command, &library, &position, &trace)
        .unwrap();
    let child = command.spawn().unwrap();
    drop(command);
    let mut owned = OwnedDaemon {
        child,
        reaped: false,
    };
    gate.wait_batch().unwrap();
    assert!(!position.exists(), "daemon has not entered its main loop");
    let rows = fs::read_to_string(&trace)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert!(rows
        .iter()
        .any(|row| row["event"] == "gate" && row["completed"] == 1));
    assert_eq!(
        rows.iter()
            .filter(|row| row["event"] == "return" && row["result"] == 0)
            .map(|row| row["completed"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [1]
    );
    assert_eq!(gate.report().completed_waits_before_input, 1);
    gate.finish().unwrap();
    rustix::process::kill_process(Pid::from_child(&owned.child), Signal::TERM).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = owned.child.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "paramsd did not stop within five seconds"
        );
        thread::sleep(Duration::from_millis(5));
    };
    owned.reaped = true;
    assert!(status.success(), "paramsd exited {status}");
    fs::write(
        evidence.join("result.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "pass":true,
            "daemon_exit":status.code(),
            "completed_waits_before_input":1,
            "child_gate_fds":child_gate_fds,
            "original_phase_hook_sha256":"1b524e78cdfa1cb4013a35cb570fd2fbc03cce7a0153a5ef8ba1627102265a3b"
        }))
        .unwrap(),
    )
    .unwrap();
}
