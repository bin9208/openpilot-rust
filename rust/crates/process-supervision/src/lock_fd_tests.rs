use super::*;
use std::{fs::OpenOptions, os::fd::AsFd};

// These tests intentionally clear CLOEXEC. Run their FD lifetime assertions in
// a separate process so concurrently spawning tests cannot inherit the lock.
fn isolated(name: &str) -> bool {
    if std::env::var("OPENPILOT_LOCK_FD_TEST").ok().as_deref() == Some(name) {
        return false;
    }
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("OPENPILOT_LOCK_FD_TEST", name)
        .status()
        .unwrap();
    assert!(
        status.success(),
        "isolated descriptor lifetime test failed: {name}"
    );
    true
}

#[test]
fn received_lock_preserves_open_file_description() {
    if isolated("lock_fd::tests::received_lock_preserves_open_file_description") {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("repo-lock");
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    file.lock().unwrap();
    let original_flags = rustix::io::fcntl_getfd(&file).unwrap();
    let (mut sender, mut receiver) = UnixStream::pair().unwrap();
    let thread = std::thread::spawn(move || receive(&mut receiver).unwrap());
    send(&mut sender, file.as_fd()).unwrap();
    let received = thread.join().unwrap();
    assert_eq!(rustix::io::fcntl_getfd(&file).unwrap(), original_flags);
    assert_eq!(
        rustix::io::fcntl_getfd(&received).unwrap(),
        rustix::io::FdFlags::empty()
    );
    drop(file);
    let contender = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    assert!(matches!(
        contender.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    drop(received);
    contender.try_lock().unwrap();
}

#[test]
fn malformed_lock_transfers_close_received_descriptors() {
    if isolated("lock_fd::tests::malformed_lock_transfers_close_received_descriptors") {
        return;
    }
    for (marker, count) in [
        (b"X".as_slice(), 1),
        (b"L", 0),
        (b"L", 2),
        (b"L", 8),
        (b"LL", 1),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("repo-lock");
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        file.lock().unwrap();
        let (sender, mut receiver) = UnixStream::pair().unwrap();
        let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(8))];
        let descriptors = vec![file.as_fd(); count];
        let mut control = SendAncillaryBuffer::new(&mut space);
        if count > 0 {
            assert!(control.push(SendAncillaryMessage::ScmRights(&descriptors)));
        }
        sendmsg(
            &sender,
            &[IoSlice::new(marker)],
            &mut control,
            SendFlags::NOSIGNAL,
        )
        .unwrap();
        assert!(
            receive(&mut receiver).is_err(),
            "marker={marker:?}, count={count}"
        );
        drop(file);
        let contender = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
        contender.try_lock().unwrap();
    }
}

#[test]
fn missing_acknowledgement_fails_closed() {
    let file = tempfile::tempfile().unwrap();
    let (mut sender, receiver) = UnixStream::pair().unwrap();
    drop(receiver);
    assert!(send(&mut sender, file.as_fd()).is_err());
}
