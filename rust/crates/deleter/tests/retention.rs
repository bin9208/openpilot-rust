use openpilot_deleter::{Deleter, MIN_BYTES, MIN_PERCENT};
use rustix::fs::{setxattr, XattrFlags};
use std::{fs, os::unix::fs::symlink, time::Duration};

fn segment(root: &std::path::Path, name: &str) {
    fs::create_dir(root.join(name)).unwrap();
    fs::write(root.join(name).join("rlog.zst"), b"synthetic").unwrap();
}

#[test]
fn low_space_skips_locks_and_defers_preserved_and_boot_directories() {
    let root = tempfile::tempdir().unwrap();
    for name in [
        "boot", "crash", "route--0", "route--1", "route--2", "route--3",
    ] {
        segment(root.path(), name);
    }
    fs::write(root.path().join("route--0/held.lock"), []).unwrap();
    setxattr(
        root.path().join("route--3"),
        "user.preserve",
        b"1",
        XattrFlags::empty(),
    )
    .unwrap();
    let mut deleter = Deleter::new(root.path());
    let idle = deleter.cycle(MIN_BYTES, MIN_PERCENT).unwrap();
    assert_eq!(idle.wait, Duration::from_secs(30));
    assert!(idle.deleted.is_none());
    let low = deleter.cycle(MIN_BYTES - 1, MIN_PERCENT).unwrap();
    assert_eq!(
        low.deleted.as_deref(),
        Some(std::ffi::OsStr::new("route--1"))
    );
    assert_eq!(low.wait, Duration::from_millis(100));
    assert!(root.path().join("route--0/held.lock").is_file());
    assert!(root.path().join("route--3").is_dir());
    assert!(root.path().join("boot").is_dir());
}

#[test]
fn symlink_candidate_is_not_unlinked_or_followed_for_removal() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("untouched"), b"outside").unwrap();
    symlink(outside.path(), root.path().join("route--0")).unwrap();
    segment(root.path(), "route--1");
    let step = Deleter::new(root.path()).cycle(0, 0.0).unwrap();
    assert_eq!(
        step.deleted.as_deref(),
        Some(std::ffi::OsStr::new("route--1"))
    );
    assert!(root.path().join("route--0").is_symlink());
    assert_eq!(
        fs::read(outside.path().join("untouched")).unwrap(),
        b"outside"
    );
}

#[test]
fn preserve_attributes_are_cached_like_the_source_process() {
    let root = tempfile::tempdir().unwrap();
    for name in ["route--0", "route--1", "route--2", "route--3"] {
        segment(root.path(), name);
    }
    let mut deleter = Deleter::new(root.path());
    assert!(deleter.preserved().unwrap().is_empty());
    setxattr(
        root.path().join("route--3"),
        "user.preserve",
        b"1",
        XattrFlags::empty(),
    )
    .unwrap();
    assert!(deleter.preserved().unwrap().is_empty());
    let fresh = Deleter::new(root.path()).preserved().unwrap();
    assert_eq!(fresh.len(), 3);
    for name in ["route--1", "route--2", "route--3"] {
        assert!(fresh.contains(std::ffi::OsStr::new(name)));
    }
}
