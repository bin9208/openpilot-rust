use openpilot_runtime_build::objects::import;
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    process::Command,
};

#[test]
fn running_candidate_is_independent_of_rewritten_cargo_output_and_reuse_copies_zero_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("mutable-cargo-output");
    fs::copy("/bin/true", &output).unwrap();
    let store = temp.path().join("objects");
    let first = import(&store, &output, 0).unwrap();
    assert!(first.stored_bytes > 0);
    let candidate = temp.path().join("candidate");
    first.object.materialize(&store, &candidate).unwrap();
    assert_ne!(
        fs::metadata(&output).unwrap().ino(),
        fs::metadata(&candidate).unwrap().ino()
    );
    let reused = import(&store, &output, u64::MAX).unwrap();
    assert_eq!(reused.object, first.object);
    assert_eq!(reused.stored_bytes, 0);
    assert!(first.object.materialize(&store, &candidate).is_err());
    fs::write(&output, "broken next compilation output").unwrap();
    first.object.verify(&store).unwrap();
    assert!(Command::new(&candidate).status().unwrap().success());
    let changed = import(&store, &output, 0).unwrap();
    assert_ne!(changed.object.sha256, first.object.sha256);
    assert!(Command::new(&candidate).status().unwrap().success());
}

#[test]
fn low_space_and_busy_object_do_not_replace_previous_candidate() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("input");
    fs::write(&input, "previous generation data").unwrap();
    let store = temp.path().join("objects");
    let previous = import(&store, &input, 0).unwrap();
    let candidate = temp.path().join("candidate");
    previous.object.materialize(&store, &candidate).unwrap();
    let inode = fs::metadata(&candidate).unwrap().ino();
    let blob = previous.object.verify(&store).unwrap();
    let lock = fs::OpenOptions::new()
        .write(true)
        .open(blob.parent().unwrap().join("lock"))
        .unwrap();
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::LockExclusive).unwrap();
    assert!(import(&store, &input, 0).is_err());
    drop(lock);
    fs::write(&input, "next generation data").unwrap();
    assert!(import(&store, &input, u64::MAX).is_err());
    assert_eq!(fs::metadata(&candidate).unwrap().ino(), inode);
    assert_eq!(fs::read(candidate).unwrap(), b"previous generation data");
    previous.object.verify(&store).unwrap();
}

#[test]
fn corrupt_or_redirected_object_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("input");
    fs::write(&input, "output data").unwrap();
    let store = temp.path().join("objects");
    let object = import(&store, &input, 0).unwrap().object;
    let blob = object.verify(&store).unwrap();
    fs::set_permissions(&blob, fs::Permissions::from_mode(0o644)).unwrap();
    fs::write(&blob, "corruption").unwrap();
    assert!(import(&store, &input, 0).is_err());
    assert!(object
        .materialize(&store, &temp.path().join("unverified"))
        .is_err());
    fs::remove_file(&blob).unwrap();
    std::os::unix::fs::symlink(&input, &blob).unwrap();
    assert!(import(&store, &input, 0).is_err());
    assert!(!temp.path().join("unverified").exists());
    fs::set_permissions(&input, fs::Permissions::from_mode(0o4755)).unwrap();
    assert!(import(&store, &input, 0).is_err());
}
