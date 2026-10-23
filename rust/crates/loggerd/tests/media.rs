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
    assert!(!root.path().join("fcamera.hevc.failed").exists());
}

// Both tests use the production stdio path. The small packet fits its buffer;
// the large one forces a real ENOSPC before close. Writes remain nonfatal so
// the daemon can record indices and finish rlog/qlog before reporting failure.
fn raw_enospc_marks_failure(payload: &[u8]) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("fcamera.hevc");
    std::os::unix::fs::symlink("/dev/full", &path).unwrap();
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
            data: payload,
            timestamp_us: 50_000,
            configuration: false,
            keyframe: true,
        })
        .unwrap();

    // Completion failure is reported per video without aborting rotation or
    // skipping the remaining encoders during normal daemon shutdown.
    writer.close().unwrap();
    assert!(!root.path().join("fcamera.hevc.lock").exists());
    assert!(root.path().join("fcamera.hevc.failed").is_file());
}

#[test]
fn marks_raw_video_failed_after_enospc_during_write() {
    raw_enospc_marks_failure(&[0x55; 8192]);
}

#[test]
fn marks_raw_video_failed_after_enospc_delayed_until_flush() {
    raw_enospc_marks_failure(b"buffered video");
}
