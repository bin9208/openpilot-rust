use openpilot_cluster::autorun::events::Events;
use std::{os::unix::net::UnixDatagram, time::Duration};

#[test]
fn event_wait_drains_owned_socket_and_retains_a_matching_event() {
    let (receiver, sender) = UnixDatagram::pair().unwrap();
    receiver.set_nonblocking(true).unwrap();
    let events = Events::from_socket(receiver.into());
    sender
        .send(b"add@/devices/owned\0SUBSYSTEM=usb\0PRODUCT=1cbe/92/1\0")
        .unwrap();
    sender
        .send(b"remove@/devices/owned\0SUBSYSTEM=usb\0PRODUCT=1cbe/92/1\0")
        .unwrap();
    assert!(events.wait(Duration::from_millis(100), 0x92));
    assert!(!events.wait(Duration::from_millis(1), 0x92));
}

#[test]
fn zero_timeout_preserves_a_pending_event() {
    let (receiver, sender) = UnixDatagram::pair().unwrap();
    receiver.set_nonblocking(true).unwrap();
    let events = Events::from_socket(receiver.into());
    sender
        .send(b"change@/devices/owned\0SUBSYSTEM=usb\0PRODUCT=1cbe/92/1\0")
        .unwrap();
    assert!(!events.wait(Duration::ZERO, 0x92));
    assert!(events.wait(Duration::from_millis(100), 0x92));
}
