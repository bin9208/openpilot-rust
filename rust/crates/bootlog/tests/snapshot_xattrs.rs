//! The original shutil.copytree contract includes file/directory xattrs and metadata.
use openpilot_bootlog::snapshot::{Error, Snapshot};
use rustix::fs::{getxattr, setxattr, XattrFlags};
use std::{
    fs,
    os::unix::fs::{symlink, MetadataExt, PermissionsExt},
    path::Path,
};
fn put(path: &Path, name: &str, bytes: &[u8]) {
    setxattr(path, name, bytes, XattrFlags::empty()).unwrap();
}
fn get(path: &Path, name: &str) -> Vec<u8> {
    let mut bytes = vec![0; 65536];
    let count = getxattr(path, name, bytes.as_mut_slice()).unwrap();
    bytes.truncate(count);
    bytes
}
#[test]
fn snapshot_preserves_recursive_xattrs_file_modes_and_nanosecond_mtime() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("params");
    fs::create_dir_all(source.join("nested")).unwrap();
    let nested = source.join("nested");
    let file = nested.join("value");
    fs::write(&file, b"before\0binary").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
    let stamp = rustix::fs::Timestamps {
        last_access: rustix::fs::Timespec {
            tv_sec: 1_600_000_000,
            tv_nsec: 123_456_789,
        },
        last_modification: rustix::fs::Timespec {
            tv_sec: 1_600_000_000,
            tv_nsec: 987_654_321,
        },
    };
    rustix::fs::utimensat(rustix::fs::CWD, &file, &stamp, rustix::fs::AtFlags::empty()).unwrap();
    for (name, value) in [
        ("user.empty", &b""[..]),
        ("user.short", &b"1"[..]),
        ("user.binary", &b"\0\xffvalue"[..]),
    ] {
        put(&file, name, value);
    }
    put(&nested, "user.directory", b"nested");
    put(&source, "user.directory", b"root");
    symlink("value", nested.join("link")).unwrap();
    let snapshot = Snapshot::capture(&source, root.path()).unwrap();
    let copy = snapshot.path().join("params/nested/value");
    assert_eq!(fs::read(&copy).unwrap(), b"before\0binary");
    assert_eq!(get(&copy, "user.empty"), b"");
    assert_eq!(get(&copy, "user.short"), b"1");
    assert_eq!(get(&copy, "user.binary"), b"\0\xffvalue");
    assert_eq!(
        get(&snapshot.path().join("params"), "user.directory"),
        b"root"
    );
    assert_eq!(
        get(&snapshot.path().join("params/nested"), "user.directory"),
        b"nested"
    );
    assert_eq!(fs::metadata(&copy).unwrap().mode() & 0o777, 0o640);
    assert_eq!(fs::metadata(&copy).unwrap().mtime(), 1_600_000_000);
    assert_eq!(fs::metadata(&copy).unwrap().mtime_nsec(), 987_654_321);
    let linked = snapshot.path().join("params/nested/link");
    assert!(!fs::symlink_metadata(&linked).unwrap().is_symlink());
    assert_eq!(get(&linked, "user.binary"), b"\0\xffvalue");
    fs::write(&file, b"changed after snapshot").unwrap();
    put(&file, "user.binary", b"changed");
    assert_eq!(fs::read(&copy).unwrap(), b"before\0binary");
    assert_eq!(get(&copy, "user.binary"), b"\0\xffvalue");
}
#[test]
fn failed_copy_keeps_good_xattrs_and_owned_failed_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("params");
    fs::create_dir(&source).unwrap();
    let file = source.join("value");
    fs::write(&file, b"good").unwrap();
    put(&file, "user.binary", b"\xff\0");
    symlink("missing", source.join("broken")).unwrap();
    let Err(Error::Copy { snapshot, failures }) = Snapshot::capture(&source, root.path()) else {
        panic!("dangling source must retain failed snapshot");
    };
    assert!(snapshot.is_dir());
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].0, source.join("broken"));
    assert_eq!(fs::read(snapshot.join("params/value")).unwrap(), b"good");
    assert_eq!(
        get(&snapshot.join("params/value"), "user.binary"),
        b"\xff\0"
    );
}
