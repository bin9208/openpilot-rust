use openpilot_bootlog::snapshot::Snapshot;
use std::{fs, os::unix::fs::symlink};

#[test]
fn snapshot_retains_bytes_after_live_params_change() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("live/d");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("DongleId"), b"before\0binary").unwrap();
    let snapshot = Snapshot::capture(&source, root.path()).unwrap();
    fs::write(source.join("DongleId"), b"after").unwrap();
    assert_eq!(
        fs::read(snapshot.path().join("d/DongleId")).unwrap(),
        b"before\0binary"
    );
    let path = snapshot.path().to_owned();
    snapshot.run(&root.path().join("missing-loggerd")).unwrap();
    assert!(!path.exists());
}

#[test]
fn dangling_param_link_prevents_launch_and_retains_failed_copy() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("live/d");
    fs::create_dir_all(&source).unwrap();
    symlink("missing", source.join("broken")).unwrap();
    assert!(Snapshot::capture(&source, root.path()).is_err());
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
}

#[test]
fn failed_child_spawn_retains_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("live/d");
    let loggerd = root.path().join("loggerd");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir(&loggerd).unwrap();
    fs::write(loggerd.join("bootlog"), b"not executable").unwrap();
    let snapshot = Snapshot::capture(&source, root.path()).unwrap();
    let path = snapshot.path().to_owned();
    assert!(snapshot.run(&loggerd).is_err());
    assert!(path.is_dir());
}
