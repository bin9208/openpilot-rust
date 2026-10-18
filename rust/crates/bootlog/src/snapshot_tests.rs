//! Real filesystem ownership regression; no allocator override or unsafe code.
use super::*;
use rustix::fs::{getxattr, setxattr, XattrFlags};

fn attribute(path: &Path, name: &str, value: &[u8]) {
    setxattr(path, name, value, XattrFlags::empty()).unwrap();
}
fn copied_attribute(path: &Path, name: &str) -> Vec<u8> {
    let mut bytes = vec![0; 65536];
    let size = getxattr(path, name, bytes.as_mut_slice()).unwrap();
    bytes.truncate(size);
    bytes
}

#[test]
fn xattr_scratch_keeps_both_allocations_across_attributes_and_recursive_copies() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    fs::create_dir_all(source.join("nested")).unwrap();
    let mut scratch = XattrScratch::default();
    assert!(scratch.names.is_empty() && scratch.value.is_empty());
    let warm = root.path().join("warm");
    fs::write(&warm, b"warm").unwrap();
    attribute(&warm, "user.warm", b"1");
    let warm_copy = root.path().join("warm-copy");
    fs::write(&warm_copy, b"").unwrap();
    copy_xattrs(&warm, &warm_copy, &mut scratch).unwrap();
    let names_pointer = scratch.names.as_ptr();
    let value_pointer = scratch.value.as_ptr();
    let names_capacity = scratch.names.capacity();
    let value_capacity = scratch.value.capacity();
    assert!(names_capacity >= 65536 && value_capacity >= 65536);
    for index in 0..16 {
        let path = source.join("nested").join(format!("param-{index}"));
        fs::write(&path, b"bytes\0retained").unwrap();
        attribute(&path, "user.empty", b"");
        attribute(&path, "user.one", b"1");
        attribute(&path, "user.binary", b"\0\xffdata");
        attribute(&path, "user.large", &vec![0xA5; 2048]);
    }
    attribute(&source, "user.directory", b"root");
    attribute(&source.join("nested"), "user.directory", b"nested");
    let target = root.path().join("target");
    let mut failures = vec![];
    copy_tree(&source, &target, &mut failures, &mut scratch);
    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(scratch.names.as_ptr(), names_pointer);
    assert_eq!(scratch.value.as_ptr(), value_pointer);
    assert_eq!(scratch.names.capacity(), names_capacity);
    assert_eq!(scratch.value.capacity(), value_capacity);
    println!(
        "xattr scratch: 16 files, 2 directories, 66 attributes; names/value capacity {names_capacity}/{value_capacity}; both allocation pointers retained"
    );
    assert_eq!(copied_attribute(&target, "user.directory"), b"root");
    assert_eq!(
        copied_attribute(&target.join("nested"), "user.directory"),
        b"nested"
    );
    for index in 0..16 {
        let path = target.join("nested").join(format!("param-{index}"));
        assert_eq!(fs::read(&path).unwrap(), b"bytes\0retained");
        assert_eq!(copied_attribute(&path, "user.empty"), b"");
        assert_eq!(copied_attribute(&path, "user.one"), b"1");
        assert_eq!(copied_attribute(&path, "user.binary"), b"\0\xffdata");
        assert_eq!(copied_attribute(&path, "user.large"), vec![0xA5; 2048]);
    }
}

#[test]
fn xattr_scratch_reuse_cannot_leak_previous_names_or_value_tail() {
    let root = tempfile::tempdir().unwrap();
    let mut scratch = XattrScratch::default();
    for (index, value) in [vec![0xB6; 2048], vec![1], vec![]].iter().enumerate() {
        let source = root.path().join(format!("source-{index}"));
        let target = root.path().join(format!("target-{index}"));
        fs::write(&source, b"source").unwrap();
        fs::write(&target, b"target").unwrap();
        attribute(&source, "user.value", value);
        copy_xattrs(&source, &target, &mut scratch).unwrap();
        assert_eq!(copied_attribute(&target, "user.value"), *value);
    }
    let source = root.path().join("no-attributes");
    let target = root.path().join("empty-target");
    fs::write(&source, b"").unwrap();
    fs::write(&target, b"").unwrap();
    copy_xattrs(&source, &target, &mut scratch).unwrap();
    assert_eq!(
        getxattr(&target, "user.value", &mut [0_u8; 8][..]),
        Err(rustix::io::Errno::NODATA)
    );
    assert_eq!(
        copy_xattrs(&root.path().join("missing"), &target, &mut scratch)
            .unwrap_err()
            .kind(),
        io::ErrorKind::NotFound
    );
}
