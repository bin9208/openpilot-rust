#![cfg(feature = "native")]
#![allow(unsafe_code)]

use openpilot_encoderd::{native::Mapping, Error};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    fs::File,
    io::Write,
    os::fd::{FromRawFd, OwnedFd},
};

thread_local! {
    static TRACK: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

struct Counter;

fn record_allocation() {
    let _ = TRACK.try_with(|track| {
        if track.get() {
            let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        }
    });
}

// SAFETY: pointer and layout arguments are passed unchanged to System; the
// thread-local counter observes only the measured calls in this test process.
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record_allocation();
        unsafe { System.realloc(pointer, layout, size) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counter = Counter;

fn mapping(bytes: &[u8]) -> Mapping {
    // SAFETY: memfd_create returns a new owned descriptor on success.
    let raw = unsafe { libc::memfd_create(c"v4l-packet-copy".as_ptr(), libc::MFD_CLOEXEC) };
    assert!(raw >= 0, "{}", std::io::Error::last_os_error());
    // SAFETY: raw was just created and has not been transferred elsewhere.
    let mut file = unsafe { File::from_raw_fd(raw) };
    file.set_len(bytes.len() as u64).unwrap();
    file.write_all(bytes).unwrap();
    let fd: OwnedFd = file.into();
    // SAFETY: the retained memfd has this length and is never truncated.
    unsafe { Mapping::new(fd, bytes.len()) }.unwrap()
}

fn fingerprint(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[test]
fn reused_owned_packet_buffer_preserves_bytes_and_removes_warmed_copy_allocations() {
    const MAX: usize = 4096;
    const LENGTHS: [usize; 9] = [0, 1, 65, MAX, 7, 1024, MAX, 0, 33];
    let source: Vec<u8> = (0..MAX)
        .map(|index| (index as u8).wrapping_mul(37))
        .collect();
    let mapping = mapping(&source);
    let mut scratch = Vec::new();
    let mut old_fingerprints = Vec::new();
    let mut new_fingerprints = Vec::new();
    for length in LENGTHS {
        let old = mapping.copy(length).unwrap();
        if scratch.len() < length {
            scratch.resize(length, 0);
        }
        mapping.copy_into(&mut scratch[..length]).unwrap();
        let published = &scratch[..length];
        assert_eq!(published, old, "packet length {length}");
        assert_eq!(published.len(), length, "do not publish the retained tail");
        if length == 7 {
            assert_eq!(scratch.len(), MAX);
            assert_ne!(fingerprint(&scratch), fingerprint(published));
        }
        old_fingerprints.push(fingerprint(&old));
        new_fingerprints.push(fingerprint(published));
    }
    assert_eq!(new_fingerprints, old_fingerprints);
    assert_eq!(scratch.len(), MAX);
    assert!(scratch.capacity() >= MAX);

    let mut old_allocations = 0;
    let mut new_allocations = 0;
    let mut old_copied_bytes = 0;
    let mut new_copied_bytes = 0;
    for _ in 0..10 {
        for length in LENGTHS.into_iter().filter(|length| *length > 0) {
            ALLOCATIONS.with(|count| count.set(0));
            TRACK.with(|track| track.set(true));
            let old = mapping.copy(length).unwrap();
            TRACK.with(|track| track.set(false));
            old_allocations += ALLOCATIONS.with(Cell::get);
            old_copied_bytes += length;
            assert_eq!(fingerprint(&old), fingerprint(&source[..length]));

            ALLOCATIONS.with(|count| count.set(0));
            TRACK.with(|track| track.set(true));
            if scratch.len() < length {
                scratch.resize(length, 0);
            }
            mapping.copy_into(&mut scratch[..length]).unwrap();
            TRACK.with(|track| track.set(false));
            new_allocations += ALLOCATIONS.with(Cell::get);
            new_copied_bytes += length;
            assert_eq!(fingerprint(&scratch[..length]), fingerprint(&old));
        }
    }
    assert_eq!(old_allocations, 70);
    assert_eq!(new_allocations, 0);
    assert_eq!(old_copied_bytes, new_copied_bytes);
    let mut canary = [0xa5; MAX + 1];
    assert!(matches!(
        mapping.copy_into(&mut canary),
        Err(Error::Contract("DMA copy exceeds mapping"))
    ));
    assert!(canary.iter().all(|byte| *byte == 0xa5));
    mapping.copy_into(&mut canary[..8]).unwrap();
    assert_eq!(canary[..8], source[..8]);
    assert!(canary[8..].iter().all(|byte| *byte == 0xa5));
    println!(
        "{{\"old_allocations\":{old_allocations},\"new_allocations\":{new_allocations},\"old_copied_bytes\":{old_copied_bytes},\"new_copied_bytes\":{new_copied_bytes},\"maximum_logical_payload\":{MAX},\"retained_capacity\":{}}}",
        scratch.capacity()
    );
}
