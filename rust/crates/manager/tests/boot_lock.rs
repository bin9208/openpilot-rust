#[test]
fn owned_lock_identity_is_checked_before_unlock() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("lock");
    let other = directory.path().join("other");
    let mut lock = Some(std::fs::File::create(&path).unwrap());
    lock.as_ref().unwrap().lock().unwrap();
    std::fs::write(&other, b"").unwrap();
    let competing = std::fs::File::open(&path).unwrap();
    assert!(competing.try_lock().is_err());
    assert!(openpilot_manager::boot_lock::release(&mut lock, &other).is_err());
    assert!(lock.is_some());
    assert!(competing.try_lock().is_err());
    openpilot_manager::boot_lock::release(&mut lock, &path).unwrap();
    assert!(lock.is_none());
    competing.try_lock().unwrap();
}
