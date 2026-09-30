use openpilot_bootlog::snapshot::Snapshot;
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
};

#[test]
fn snapshot_retains_bytes_after_live_params_change() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("live/d");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("DongleId"), b"before\0binary").unwrap();
    let snapshot = Snapshot::capture(&source, root.path()).unwrap();
    assert_eq!(
        fs::metadata(snapshot.path()).unwrap().permissions().mode() & 0o777,
        0o700
    );
    fs::write(source.join("DongleId"), b"after").unwrap();
    assert_eq!(
        fs::read(snapshot.path().join("d/DongleId")).unwrap(),
        b"before\0binary"
    );
    let path = snapshot.path().to_owned();
    snapshot
        .run(
            &root.path().join("missing-loggerd"),
            &root.path().join("unused-helper"),
        )
        .unwrap();
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
    assert!(snapshot
        .run(&loggerd, &root.path().join("missing-helper"))
        .is_err());
    assert!(path.is_dir());
}
#[test]
fn readable_character_device_link_is_copied_as_a_regular_file() {
    let root = tempfile::tempdir().unwrap();
    let params = root.path().join("params");
    std::fs::create_dir(&params).unwrap();
    std::os::unix::fs::symlink("/dev/null", params.join("null-link")).unwrap();
    let snapshot = openpilot_bootlog::snapshot::Snapshot::capture(&params, root.path()).unwrap();
    let copied = snapshot.path().join("params/null-link");
    assert!(std::fs::symlink_metadata(&copied).unwrap().is_file());
    assert!(std::fs::read(copied).unwrap().is_empty());
}
