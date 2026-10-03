use super::*;
use std::{
    fs::File,
    os::{fd::AsFd, unix::fs::FileExt},
};

fn pair() -> (OwnedFd, OwnedFd) {
    let mut descriptors = [-1; 2];
    // SAFETY: the array covers both returned FDs; successful socketpair creates
    // two unique descriptors transferred exactly once into the owners below.
    assert_eq!(
        unsafe {
            libc::socketpair(
                libc::AF_UNIX,
                libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC,
                0,
                descriptors.as_mut_ptr(),
            )
        },
        0
    );
    // SAFETY: the successful socketpair call produced these two owned live FDs.
    unsafe {
        (
            OwnedFd::from_raw_fd(descriptors[0]),
            OwnedFd::from_raw_fd(descriptors[1]),
        )
    }
}

#[test]
fn transferred_descriptor_keeps_storage_alive_after_sender_closes() {
    let (sender, receiver) = pair();
    let file = tempfile::tempfile().unwrap();
    file.set_len(104).unwrap();
    file.write_all_at(b"camera", 0).unwrap();
    let descriptor = file.as_fd().try_clone_to_owned().unwrap();
    send(sender.as_fd(), b"header", &[descriptor]).unwrap();
    drop(file);

    let (payload, mut descriptors) = receive(receiver.as_fd(), 16).unwrap();

    assert_eq!(payload, b"header");
    assert_eq!(descriptors.len(), 1);
    let descriptor = descriptors.pop().unwrap();
    // SAFETY: fcntl F_GETFD only inspects the borrowed descriptor's flags.
    assert_ne!(
        unsafe { libc::fcntl(descriptor.as_raw_fd(), libc::F_GETFD) } & libc::FD_CLOEXEC,
        0
    );
    let file = File::from(descriptor);
    let mut bytes = [0; 6];
    file.read_exact_at(&mut bytes, 0).unwrap();
    assert_eq!(&bytes, b"camera");
}

#[test]
fn truncated_payload_closes_every_transferred_descriptor() {
    let (sender, receiver) = pair();
    let file = tempfile::tempfile().unwrap();
    let target = std::fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd())).unwrap();
    let descriptors: Vec<_> = (0..MAX_FDS)
        .map(|_| file.as_fd().try_clone_to_owned().unwrap())
        .collect();
    send(sender.as_fd(), b"too long", &descriptors).unwrap();
    drop(descriptors);

    let result = receive(receiver.as_fd(), 2);

    assert!(matches!(result, Err(Error::Corrupt(_))));
    let matching = std::fs::read_dir("/proc/self/fd")
        .unwrap()
        .filter_map(Result::ok)
        .filter_map(|entry| std::fs::read_link(entry.path()).ok())
        .filter(|path| path == &target)
        .count();
    assert_eq!(matching, 1);
}

#[test]
fn maximum_descriptor_packet_roundtrips_without_losing_rights() {
    let (sender, receiver) = pair();
    let file = tempfile::tempfile().unwrap();
    let descriptors: Vec<_> = (0..MAX_FDS)
        .map(|_| file.as_fd().try_clone_to_owned().unwrap())
        .collect();
    let bytes = vec![0xa5; MAX_FDS * crate::vision_wire::BUFFER_BYTES];
    send(sender.as_fd(), &bytes, &descriptors).unwrap();

    let (payload, received) = receive(receiver.as_fd(), bytes.len()).unwrap();

    assert_eq!(payload, bytes);
    assert_eq!(received.len(), MAX_FDS);
}
