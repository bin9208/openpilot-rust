#![cfg(not(feature = "visionipc-ion"))]
use openpilot_msgq::{VisionLayout, VisionServer, VisionStream};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
thread_local! {static TRACK:Cell<bool>=const{Cell::new(false)};static BYTES:Cell<usize>=const{Cell::new(0)};}
struct Counter;
fn record(size: usize) {
    let _ = TRACK.try_with(|active| {
        if active.get() {
            let _ = BYTES.try_with(|bytes| bytes.set(bytes.get() + size));
        }
    });
}
// SAFETY: the allocator delegates all pointers/layouts unchanged to System.
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(size);
        unsafe { System.realloc(pointer, layout, size) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Counter = Counter;
fn measured<T>(operation: impl FnOnce() -> T) -> (T, usize) {
    BYTES.with(|bytes| bytes.set(0));
    TRACK.with(|track| track.set(true));
    let result = operation();
    TRACK.with(|track| track.set(false));
    (result, BYTES.with(Cell::get))
}
#[test]
fn producer_samples_copy_only_owned_selected_bytes_without_image_allocation() {
    if std::env::var_os("VISION_SAMPLE_CHILD").is_none() {
        let directory = tempfile::Builder::new()
            .prefix("msgq_samples-")
            .tempdir_in("/dev/shm")
            .unwrap();
        let prefix = directory
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .strip_prefix("msgq_")
            .unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "producer_samples_copy_only_owned_selected_bytes_without_image_allocation",
                "--nocapture",
            ])
            .env("VISION_SAMPLE_CHILD", "1")
            .env("OPENPILOT_PREFIX", prefix)
            .env_remove("CEREAL_FAKE")
            .status()
            .unwrap();
        assert!(result.success());
        return;
    }
    let server = VisionServer::new("sample-producer").unwrap();
    let layout = VisionLayout {
        width: 14,
        height: 10,
        stride: 24,
        uv_offset: 240,
        len: 360,
    };
    let images = server.create_stream(VisionStream::Road, 1, layout).unwrap();
    let bytes: Vec<_> = (0..layout.len)
        .map(|index| (index * 37 % 256) as u8)
        .collect();
    images[0].write(0, &bytes).unwrap();
    let (mut full, baseline) = measured(|| vec![0; layout.len]);
    images[0].copy_into(&mut full).unwrap();
    let indices = (0..10)
        .step_by(2)
        .flat_map(|row| (1..14).step_by(2).map(move |x| row * 14 + x));
    let (mut samples, allocated) = measured(|| vec![0; indices.clone().count()]);
    let (result, warm_allocated) =
        measured(|| images[0].copy_samples(indices.clone(), &mut samples));
    result.unwrap();
    assert_eq!(
        samples,
        indices.map(|offset| full[offset]).collect::<Vec<_>>()
    );
    assert_eq!(baseline, 360);
    assert_eq!(allocated, 35);
    assert_eq!(warm_allocated, 0);
    images[0].write(0, &vec![255; layout.len]).unwrap();
    assert_eq!(
        samples[0], bytes[1],
        "owned samples must not alias subsequently reused mapped image"
    );
}
