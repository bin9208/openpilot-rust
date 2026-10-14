#![cfg(not(feature = "visionipc-ion"))]

use openpilot_driving_modeld::camera::Source;
use openpilot_modeld::camera::{receive_pair, CameraSource};
use openpilot_msgq::{
    VisionClient, VisionImage, VisionLayout, VisionMetadata, VisionServer, VisionStream,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};

const IMAGE_BYTES: usize = 65_536;
static COUNTING: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
struct CountImages;
// SAFETY: every operation delegates unchanged pointers/layouts to System; counters own no memory.
unsafe impl GlobalAlloc for CountImages {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count(size);
        unsafe { System.realloc(pointer, layout, size) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}
fn count(size: usize) {
    if size >= IMAGE_BYTES && COUNTING.load(Ordering::Relaxed) {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    }
}
#[global_allocator]
static ALLOCATOR: CountImages = CountImages;

fn layout(len: usize) -> VisionLayout {
    VisionLayout {
        width: 128,
        height: 128,
        stride: 128,
        uv_offset: 16_384,
        len,
    }
}
fn publish(image: &VisionImage, bytes: &[u8], id: u32, sof: u64) {
    image.write(0, bytes).unwrap();
    let layout = layout(bytes.len());
    image
        .publish(VisionMetadata {
            width: layout.width,
            height: layout.height,
            stride: layout.stride,
            uv_offset: layout.uv_offset,
            len: layout.len,
            frame_id: id,
            timestamp_sof: sof,
            timestamp_eof: sof + 1_000_000,
            valid: true,
            received: true,
            index: 0,
            fd: -1,
        })
        .unwrap();
}
fn source(name: &str, stream: VisionStream, conflate: bool) -> Source {
    let mut client = VisionClient::new(name, stream, conflate).unwrap();
    assert!(client.connect().unwrap());
    Source::new(client, Arc::new(AtomicBool::new(false)))
}

#[test]
fn snapshots_reuse_storage_without_aliasing_pending_camera_pairs() {
    if std::env::var_os("DRIVING_CAMERA_TEST_CHILD").is_none() {
        let directory = tempfile::Builder::new()
            .prefix("msgq_drive-reuse-")
            .tempdir_in("/dev/shm")
            .unwrap();
        let namespace = directory
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .strip_prefix("msgq_")
            .unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "snapshots_reuse_storage_without_aliasing_pending_camera_pairs",
                "--nocapture",
            ])
            .env("DRIVING_CAMERA_TEST_CHILD", "1")
            .env("OPENPILOT_PREFIX", namespace)
            .env_remove("CEREAL_FAKE")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    let server = VisionServer::new("driving-reuse").unwrap();
    let main_images = server
        .create_stream(VisionStream::Road, 2, layout(IMAGE_BYTES))
        .unwrap();
    let extra_images = server
        .create_stream(VisionStream::WideRoad, 2, layout(IMAGE_BYTES))
        .unwrap();
    server.start_listener().unwrap();
    let mut main = source("driving-reuse", VisionStream::Road, true);
    let mut extra = source("driving-reuse", VisionStream::WideRoad, false);
    let main_bytes = vec![17; IMAGE_BYTES];
    let extra_bytes = vec![83; IMAGE_BYTES];
    publish(&main_images[0], &main_bytes, 1, 100_000_000);
    publish(&extra_images[0], &extra_bytes, 1, 100_000_000);
    let retained = receive_pair(&mut main, Some(&mut extra)).unwrap().unwrap();

    // Reusing the producer slot and advancing the lagging extra stream must not mutate retained snapshots.
    publish(&main_images[0], &extra_bytes, 3, 200_000_000);
    publish(&extra_images[0], &main_bytes, 2, 150_000_000);
    publish(&extra_images[1], &extra_bytes, 3, 200_000_000);
    let advanced = receive_pair(&mut main, Some(&mut extra)).unwrap().unwrap();
    assert_eq!(advanced.main().metadata.frame_id, 3);
    assert_eq!(advanced.extra().metadata.frame_id, 3);
    assert_eq!(retained.main().buffer.bytes, main_bytes);
    assert_eq!(retained.extra().buffer.bytes, extra_bytes);
    assert_ne!(
        retained.main().buffer.bytes.as_ptr(),
        advanced.main().buffer.bytes.as_ptr()
    );
    drop(retained);
    drop(advanced);

    ALLOCATIONS.store(0, Ordering::Relaxed);
    for id in 4..24 {
        publish(&main_images[0], &main_bytes, id, u64::from(id) * 50_000_000);
        publish(
            &extra_images[0],
            &extra_bytes,
            id,
            u64::from(id) * 50_000_000,
        );
        COUNTING.store(true, Ordering::Relaxed);
        let pair = receive_pair(&mut main, Some(&mut extra)).unwrap().unwrap();
        COUNTING.store(false, Ordering::Relaxed);
        assert_eq!(pair.main().buffer.bytes, main_bytes);
        assert_eq!(pair.extra().buffer.bytes, extra_bytes);
        assert_eq!(pair.main().metadata.frame_id, id);
        assert_eq!(
            pair.extra().metadata.timestamp_sof,
            u64::from(id) * 50_000_000
        );
    }
    let allocations = ALLOCATIONS.load(Ordering::Relaxed);
    assert_eq!(
        allocations, 0,
        "40 warmed-up camera snapshots must reuse their image allocations"
    );

    // A manually reconnected client may have a different allocation length; no old tail may leak.
    publish(&main_images[0], &main_bytes, 24, 1_200_000_000);
    let old_layout = main.receive().unwrap().unwrap();
    let resized = VisionServer::new("driving-resized").unwrap();
    let images = resized
        .create_stream(VisionStream::Road, 1, layout(2 * IMAGE_BYTES))
        .unwrap();
    resized.start_listener().unwrap();
    main.client = source("driving-resized", VisionStream::Road, true).client;
    let large_bytes = vec![129; 2 * IMAGE_BYTES];
    publish(&images[0], &large_bytes, 24, 1_200_000_000);
    let large = main.receive().unwrap().unwrap();
    assert_eq!(large.buffer.layout.len, 2 * IMAGE_BYTES);
    assert_eq!(large.buffer.bytes, large_bytes);
    assert_eq!(old_layout.buffer.bytes, main_bytes);
    drop(old_layout);
    drop(large);
    main.client = source("driving-reuse", VisionStream::Road, true).client;
    publish(&main_images[0], &main_bytes, 25, 1_250_000_000);
    let small = main.receive().unwrap().unwrap();
    drop(main);
    assert_eq!(small.buffer.bytes, main_bytes);
    assert_eq!(small.buffer.layout.len, IMAGE_BYTES);
}

#[test]
fn camera_generation_change_requires_fresh_source_for_each_driving_stream() {
    if std::env::var_os("DRIVING_CAMERA_RESTART_CHILD").is_none() {
        let directory = tempfile::Builder::new()
            .prefix("msgq_drive-restart-")
            .tempdir_in("/dev/shm")
            .unwrap();
        let namespace = directory
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .strip_prefix("msgq_")
            .unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "camera_generation_change_requires_fresh_source_for_each_driving_stream",
                "--nocapture",
            ])
            .env("DRIVING_CAMERA_RESTART_CHILD", "1")
            .env("OPENPILOT_PREFIX", namespace)
            .env_remove("CEREAL_FAKE")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    for (name, stream) in [
        ("restart-main", VisionStream::Road),
        ("restart-wide", VisionStream::WideRoad),
    ] {
        let server = VisionServer::new(name).unwrap();
        let images = server
            .create_stream(stream, 1, layout(IMAGE_BYTES))
            .unwrap();
        server.start_listener().unwrap();
        let mut old = source(name, stream, true);
        assert!(
            old.receive().unwrap().is_none(),
            "ordinary timeout remains nonterminal"
        );
        publish(&images[0], &vec![17; IMAGE_BYTES], 1, 100_000_000);
        let retained = old.receive().unwrap().unwrap();
        drop(images);
        drop(server);

        let replacement = VisionServer::new(name).unwrap();
        let images = replacement
            .create_stream(stream, 1, layout(2 * IMAGE_BYTES))
            .unwrap();
        replacement.start_listener().unwrap();
        for _ in 0..10 {
            publish(&images[0], &vec![83; 2 * IMAGE_BYTES], 2, 200_000_000);
            let result = old.receive();
            if !old.client.is_connected() {
                assert!(
                    result.is_err(),
                    "changed server generation must exit modeld"
                );
                break;
            }
            assert!(result.unwrap().is_none());
        }
        assert!(!old.client.is_connected());
        assert_eq!(retained.buffer.bytes, vec![17; IMAGE_BYTES]);
        assert!(
            old.receive().is_err(),
            "disconnected source must remain terminal"
        );
        old.stop.store(true, Ordering::Relaxed);
        assert!(
            old.receive().unwrap().is_none(),
            "shutdown remains graceful"
        );
        let mut fresh = source(name, stream, true);
        publish(&images[0], &vec![83; 2 * IMAGE_BYTES], 3, 300_000_000);
        let frame = fresh.receive().unwrap().unwrap();
        assert_eq!(frame.buffer.layout.len, 2 * IMAGE_BYTES);
        assert_eq!(frame.buffer.bytes, vec![83; 2 * IMAGE_BYTES]);
        assert_eq!(frame.metadata.frame_id, 3);
        drop(frame);
        // The trusted local transport still rejects an out-of-range buffer slot.
        let stream_id = if stream == VisionStream::Road { 0 } else { 2 };
        let mut invalid = openpilot_msgq::Publisher::for_runtime(
            &format!("visionipc_{name}_{stream_id}"),
            1024 * 1024,
        )
        .unwrap();
        let mut packet = [0_u8; 48];
        packet[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
        for _ in 0..10 {
            invalid.send(&packet).unwrap();
            let result = fresh.receive();
            if !fresh.client.is_connected() {
                assert!(result.is_err(), "invalid buffer index must exit modeld");
                break;
            }
            assert!(result.unwrap().is_none());
        }
        assert!(!fresh.client.is_connected());
    }
}
