#![cfg(all(feature = "native-skip-miri", target_os = "linux"))]

// Compile the private camera adapter itself, with its real native dependencies.
// No transport, pixel source, snapshot encoder or admission result is mocked.
pub use openpilot_xiaoge::{inference, nv12, service, Error};
#[allow(dead_code)]
mod native {
    pub use openpilot_xiaoge::native::{platform, shared, Error};
    pub mod vision {
        pub mod camera {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/native/vision/camera.rs"
            ));
        }
    }
}

use native::{shared::Shared, vision::camera::Camera};
use openpilot_msgq::{VisionLayout, VisionMetadata, VisionServer, VisionStream};
use openpilot_xiaoge::{config::Config, service::state::Model, service::State};
use std::{
    env, fs,
    path::PathBuf,
    process::Command,
    sync::{atomic::AtomicBool, Condvar, Mutex},
};

struct Namespace(PathBuf);
impl Drop for Namespace {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn isolated_child() -> bool {
    if env::var_os("XIAOGE_PIXEL_TEST_CHILD").is_some() {
        return false;
    }
    let prefix = format!("xiaoge-pixels-{}", std::process::id());
    let namespace = Namespace(PathBuf::from(format!("/dev/shm/msgq_{prefix}")));
    fs::create_dir(&namespace.0).unwrap();
    let binary = env::current_exe().unwrap();
    let mut command = if let Some(qemu) = env::var_os("IPC194_TEST_QEMU") {
        let mut command = Command::new(qemu);
        command
            .arg("-L")
            .arg(env::var_os("IPC194_TEST_SYSROOT").unwrap())
            .arg(binary);
        command
    } else {
        Command::new(binary)
    };
    let status = command
        .args([
            "--exact",
            "lazy_pixels_skip_unneeded_frames_and_share_one_snapshot_inference_copy",
            "--nocapture",
        ])
        .env("XIAOGE_PIXEL_TEST_CHILD", "1")
        .env("OPENPILOT_PREFIX", prefix)
        .env_remove("CEREAL_FAKE")
        .status()
        .unwrap();
    assert!(status.success(), "isolated native camera regression failed");
    true
}

fn shared() -> Shared {
    Shared {
        state: Mutex::new(State::new(
            Config::default(),
            ["classifier", "lane"].map(|path| Model {
                path: path.to_owned(),
                loaded: false,
                error: String::new(),
            }),
            1.0,
        )),
        snapshot: Condvar::new(),
        vasm_operation: Mutex::new(()),
        params: None,
        config_path: PathBuf::new(),
        running: AtomicBool::new(true),
    }
}

fn pixels(y: u8) -> Vec<u8> {
    // 8x4 visible image, padded rows, allocation gap and NV12 chroma.
    let mut bytes = vec![0xee; 112];
    for row in 0..4 {
        bytes[row * 16..row * 16 + 8].fill(y);
    }
    for row in 0..2 {
        bytes[80 + row * 16..88 + row * 16].fill(128);
    }
    bytes
}

fn metadata(id: u32) -> VisionMetadata {
    VisionMetadata {
        width: 8,
        height: 4,
        stride: 16,
        uv_offset: 80,
        len: 112,
        frame_id: id,
        timestamp_sof: u64::from(id) * 1000,
        timestamp_eof: u64::from(id) * 1000 + 100,
        valid: true,
        received: true,
        index: 0,
        fd: -1,
    }
}

#[test]
fn lazy_pixels_skip_unneeded_frames_and_share_one_snapshot_inference_copy() {
    if isolated_child() {
        return;
    }
    openpilot_opencv_runtime::initialize(2).unwrap();
    let server = VisionServer::new("camerad").unwrap();
    let layout = VisionLayout {
        width: 8,
        height: 4,
        stride: 16,
        uv_offset: 80,
        len: 112,
    };
    let road = server.create_stream(VisionStream::Road, 2, layout).unwrap();
    let wide = server
        .create_stream(VisionStream::WideRoad, 2, layout)
        .unwrap();
    server.start_listener().unwrap();
    let shared = shared();

    for (stream, images) in [(service::Stream::Road, road), (service::Stream::Wide, wide)] {
        let mut camera = Camera::new(stream);
        camera
            .receive(&shared, |_| {
                panic!("timeout must not reuse a previous frame")
            })
            .unwrap();

        images[0].write(0, &pixels(40)).unwrap();
        images[0].publish(metadata(1)).unwrap();
        let mut received = false;
        camera
            .receive(&shared, |_| {
                received = true;
                // A model/interval/BSD gate rejected inference; no snapshot pending.
                Ok(())
            })
            .unwrap();
        assert!(received, "the skipped frame must still be consumed");
        assert!(
            camera.bytes.is_empty(),
            "admission must precede pixel copying"
        );

        for (id, y, snapshot_first) in [(2, 64, true), (3, 160, false)] {
            let index = (id as usize) % 2;
            let original = pixels(y);
            images[index].write(0, &original).unwrap();
            images[index].publish(metadata(id)).unwrap();
            let expected_packed: Vec<_> = [vec![y; 32], vec![128; 16]].concat();
            let mut consumed = false;
            camera
                .receive(&shared, |frame| {
                    consumed = true;
                    let jpeg = if snapshot_first {
                        Some(frame.snapshot()?)
                    } else {
                        None
                    };
                    assert_eq!(frame.frame()?.pack()?, expected_packed);
                    let jpeg = match jpeg {
                        Some(jpeg) => jpeg,
                        None => frame.snapshot()?,
                    };
                    assert!(jpeg.starts_with(&[0xff, 0xd8]));
                    // Producer recycling is legal even while VisionFrame is held.
                    // Once copied, both consumers must retain the same owned image.
                    images[index].write(0, &pixels(220)).unwrap();
                    assert_eq!(frame.frame()?.pack()?, expected_packed);
                    assert_eq!(frame.snapshot()?.as_ref(), jpeg.as_ref());
                    Ok(())
                })
                .unwrap();
            assert!(consumed);
            assert_eq!(camera.bytes, original);

            images[index].write(0, &pixels(12)).unwrap();
            images[index].publish(metadata(id + 10)).unwrap();
            let mut skipped = false;
            camera
                .receive(&shared, |_| {
                    skipped = true;
                    Ok(())
                })
                .unwrap();
            assert!(skipped);
            assert_eq!(
                camera.bytes, original,
                "a later skipped frame copied pixels"
            );
        }
        camera
            .receive(&shared, |_| {
                panic!("timeout must not expose retained pixels")
            })
            .unwrap();
    }
}
