use openpilot_cereal::log_capnp::encode_index::Type;
use openpilot_loggerd::media::{Packet, VideoSpec, VideoWriter};
use std::fs;

#[test]
fn retains_incomplete_muxer_lock_when_no_header_was_written() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("qcamera.ts");
    let writer = VideoWriter::open(
        &path,
        VideoSpec {
            width: 160,
            height: 120,
            fps: 20,
            codec: Type::QcameraH264,
        },
    )
    .unwrap();

    let result = writer.close();

    assert!(result.is_err());
    assert_eq!(fs::metadata(path).unwrap().len(), 0);
    assert!(root.path().join("qcamera.ts.lock").exists());
}

#[test]
fn preserves_raw_header_and_payload_bytes_when_hevc_writer_closes() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("fcamera.hevc");
    let mut writer = VideoWriter::open(
        &path,
        VideoSpec {
            width: 160,
            height: 120,
            fps: 20,
            codec: Type::FullHEVC,
        },
    )
    .unwrap();
    writer
        .write(Packet {
            data: &[0, 0, 0, 1, 0x40],
            timestamp_us: 50_000,
            configuration: true,
            keyframe: false,
        })
        .unwrap();
    writer
        .write(Packet {
            data: &[0, 0, 1, 0x26, 0xff],
            timestamp_us: 50_000,
            configuration: false,
            keyframe: true,
        })
        .unwrap();

    writer.close().unwrap();

    assert_eq!(
        fs::read(path).unwrap(),
        [0, 0, 0, 1, 0x40, 0, 0, 1, 0x26, 0xff]
    );
    assert!(!root.path().join("fcamera.hevc.lock").exists());
}
