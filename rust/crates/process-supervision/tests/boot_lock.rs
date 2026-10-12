use std::{
    fs::{File, TryLockError},
    os::{fd::AsFd, unix::net::UnixStream},
    process::Command,
    thread,
};

#[test]
fn manager_receives_same_lock_without_leaking_it_to_daemons() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("lock");
    let original = File::create(&path).unwrap();
    original.lock().unwrap();
    let (mut sender, mut receiver) = UnixStream::pair().unwrap();
    let sending = thread::spawn(move || {
        openpilot_process_supervision::send_boot_lock(&mut sender, original.as_fd()).unwrap();
    });
    let received = openpilot_process_supervision::receive_boot_lock(&mut receiver).unwrap();
    sending.join().unwrap();
    let contender = File::open(&path).unwrap();
    assert!(matches!(
        contender.try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    let mut daemon = Command::new("sleep").arg("30").spawn().unwrap();
    drop(received);
    let lock_result = contender.try_lock();
    daemon.kill().unwrap();
    daemon.wait().unwrap();
    assert!(lock_result.is_ok());
}
