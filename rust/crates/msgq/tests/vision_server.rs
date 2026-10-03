#![cfg(feature = "native-skip-miri")]

mod support;

use openpilot_msgq::{
    RawVisionImage, VisionClient, VisionLayout, VisionMetadata, VisionServer, VisionStream,
};
use std::{env, os::fd::AsFd, thread, time::Duration};

#[test]
fn camera_server_preserves_frames_and_buffer_owners() {
    if env::var_os("RUST_VISION_SERVER_TEST_CHILD").is_none() {
        let namespace = tempfile::Builder::new()
            .prefix("msgq_rust-camera-")
            .tempdir_in("/dev/shm")
            .unwrap();
        let prefix = namespace.path().file_name().unwrap().to_str().unwrap();
        let status = support::command(env::current_exe().unwrap())
            .args([
                "--exact",
                "camera_server_preserves_frames_and_buffer_owners",
                "--nocapture",
            ])
            .env("RUST_VISION_SERVER_TEST_CHILD", "1")
            .env("OPENPILOT_PREFIX", prefix.strip_prefix("msgq_").unwrap())
            .env_remove("CEREAL_FAKE")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    let before = std::fs::read_dir("/proc/self/fd").unwrap().count();
    let layout = VisionLayout {
        width: 8,
        height: 4,
        stride: 16,
        uv_offset: 64,
        len: 96,
    };
    let server = VisionServer::new("rustcamera").unwrap();
    assert!(server.create_stream(VisionStream::Road, 0, layout).is_err());
    let images = server.create_stream(VisionStream::Road, 4, layout).unwrap();
    assert!(server.create_stream(VisionStream::Road, 4, layout).is_err());
    server.start_listener().unwrap();
    assert!(server.start_listener().is_err());
    assert!(server
        .create_stream(VisionStream::Driver, 4, layout)
        .is_err());
    for _ in 0..200 {
        if !VisionClient::available_streams("rustcamera")
            .unwrap()
            .is_empty()
        {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    let mut client = VisionClient::new("rustcamera", VisionStream::Road, false).unwrap();
    assert!(client.connect().unwrap());
    let fd = images[2].as_fd().try_clone_to_owned().unwrap();
    drop(fd);
    let data: Vec<_> = (0_u8..96).map(|n| n.wrapping_add(23)).collect();
    assert!(images[2].write(1, &data).is_err());
    images[2].write(0, &data).unwrap();
    let metadata = VisionMetadata {
        width: 8,
        height: 4,
        stride: 16,
        uv_offset: 64,
        len: 96,
        frame_id: 23,
        timestamp_sof: 23000,
        timestamp_eof: 23100,
        valid: false,
        received: true,
        index: 2,
        fd: -1,
    };
    images[2].publish(metadata).unwrap();
    let frame = client.receive(Duration::from_secs(2)).unwrap().unwrap();
    assert_eq!(frame.metadata().frame_id, 23);
    assert_eq!(frame.metadata().timestamp_sof, 23000);
    assert_eq!(frame.metadata().timestamp_eof, 23100);
    assert!(!frame.metadata().valid);
    assert_eq!(frame.metadata().index, 2);
    assert!(frame.metadata().fd >= 0);
    let mut copied = vec![0; 96];
    frame.copy_into(&mut copied).unwrap();
    assert_eq!(copied, data);
    drop(server);
    images[2].copy_into(&mut copied).unwrap();
    assert_eq!(copied, data);
    images[2]
        .publish(VisionMetadata {
            frame_id: 24,
            valid: true,
            ..metadata
        })
        .unwrap();
    assert_eq!(
        client
            .receive(Duration::from_secs(2))
            .unwrap()
            .unwrap()
            .metadata()
            .frame_id,
        24
    );
    drop(client);
    drop(images);
    assert!(VisionClient::available_streams("rustcamera")
        .unwrap()
        .is_empty());
    let raw = RawVisionImage::new(64).unwrap();
    assert!(RawVisionImage::new(0).is_err());
    assert!(RawVisionImage::new(63).is_err());
    raw.write(56, &[1; 8]).unwrap();
    assert!(raw.write(57, &[1; 8]).is_err());
    assert!(raw.write(usize::MAX, &[1]).is_err());
    let mut bytes = [0; 64];
    raw.copy_into(&mut bytes).unwrap();
    assert_eq!(&bytes[56..], &[1; 8]);
    let fd = raw.as_fd().try_clone_to_owned().unwrap();
    drop(fd);
    drop(raw);
    assert_eq!(std::fs::read_dir("/proc/self/fd").unwrap().count(), before);
}
