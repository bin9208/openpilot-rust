use openpilot_dev_tools::paramsd_startup_gate::StartupGate;
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    io::{read, write},
};
use std::{fs, path::Path};

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
