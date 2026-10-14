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
fn bookmark_added_after_a_scan_preserves_the_segment_and_two_prior() {
    let root = tempfile::tempdir().unwrap();
    for name in ["route--0", "route--1", "route--2", "route--3"] {
        segment(root.path(), name);
    }
    let mut deleter = Deleter::new(root.path());
    fs::write(root.path().join("route--2/rlog.lock"), []).unwrap();
    assert!(deleter.preserved().unwrap().is_empty());
    setxattr(
        root.path().join("route--2"),
        "user.preserve",
        b"1",
        XattrFlags::empty(),
    )
    .unwrap();
    fs::remove_file(root.path().join("route--2/rlog.lock")).unwrap();
    let low = deleter.cycle(0, 0.0).unwrap();
    assert_eq!(
        low.deleted.as_deref(),
        Some(std::ffi::OsStr::new("route--3"))
    );
    for name in ["route--0", "route--1", "route--2"] {
        assert!(root.path().join(name).is_dir());
    }
}

#[test]
fn removed_bookmark_no_longer_defers_segment_deletion() {
    let root = tempfile::tempdir().unwrap();
    for name in ["route--0", "route--1"] {
        segment(root.path(), name);
    }
    let path = root.path().join("route--0");
    setxattr(&path, "user.preserve", b"1", XattrFlags::empty()).unwrap();
    let mut deleter = Deleter::new(root.path());
    assert!(deleter
        .preserved()
        .unwrap()
        .contains(std::ffi::OsStr::new("route--0")));
    rustix::fs::removexattr(&path, "user.preserve").unwrap();
    let low = deleter.cycle(0, 0.0).unwrap();
    assert_eq!(
        low.deleted.as_deref(),
        Some(std::ffi::OsStr::new("route--0"))
    );
    assert!(root.path().join("route--1").is_dir());
}

#[test]
fn only_the_exact_preserve_marker_defers_deletion() {
    let root = tempfile::tempdir().unwrap();
    for (name, marker) in [
        ("route--0", b"01".as_slice()),
        ("route--1", b""),
        ("route--2", b"1\0"),
    ] {
        segment(root.path(), name);
        setxattr(
            root.path().join(name),
            "user.preserve",
            marker,
            XattrFlags::empty(),
        )
        .unwrap();
    }
    let mut deleter = Deleter::new(root.path());
    assert!(deleter.preserved().unwrap().is_empty());
    assert_eq!(
        deleter.cycle(0, 0.0).unwrap().deleted.as_deref(),
        Some(std::ffi::OsStr::new("route--0"))
    );
}
