use openpilot_cluster::media::HardwareFrame;
use openpilot_startup_ui::egl::FrameLayout;
use std::{
    os::fd::AsFd,
    sync::{mpsc, Arc},
};

#[test]
fn retained_frame_identifies_decoder_generation_and_notifies_import_failure() {
    use openpilot_cluster::media::frame::FrameLease;
    let (release, _released) = mpsc::channel();
    let (failures, failed) = mpsc::channel();
    let frame = HardwareFrame::retained(
        tempfile::tempfile().unwrap().into(),
        FrameLayout {
            width: 8,
            height: 4,
            stride: 16,
            uv_offset: 64,
        },
        FrameLease {
            index: 3,
            generation: 7,
            sequence: u64::MAX,
            release,
            failures,
        },
    )
    .unwrap();
    assert_eq!(frame.token(), (7, 3));
    assert_eq!(frame.sequence(), u64::MAX);
    frame.mark_egl_import_failed("source failed EGL import");
    assert_eq!(failed.recv().unwrap(), "source failed EGL import");
    frame.mark_egl_import_failed("");
    assert_eq!(failed.recv().unwrap(), "TICI H264 EGL import failed");
    println!(
        "retained frame native token {:?}, sequence {} and exact fallback notification",
        frame.token(),
        frame.sequence()
    );
}

#[test]
fn decoder_buffer_is_released_only_after_last_owned_frame() {
    let file = tempfile::tempfile().unwrap();
    file.set_len(96).unwrap();
    let (release, received) = mpsc::channel();
    let frame = HardwareFrame::new(
        file.into(),
        FrameLayout {
            width: 8,
            height: 4,
            stride: 16,
            uv_offset: 64,
        },
        3,
        release,
    )
    .unwrap();
    let retained = Arc::clone(&frame);
    assert!(rustix::fs::fstat(frame.as_fd()).is_ok());
    drop(frame);
    assert!(received.try_recv().is_err());
    assert!(rustix::fs::fstat(retained.as_fd()).is_ok());
    drop(retained);
    assert_eq!(received.recv().unwrap(), 3);
}

#[test]
fn invalid_nv12_layout_does_not_create_a_decoder_lease() {
    for layout in [
        FrameLayout {
            width: 7,
            height: 4,
            stride: 16,
            uv_offset: 64,
        },
        FrameLayout {
            width: 8,
            height: 3,
            stride: 16,
            uv_offset: 64,
        },
        FrameLayout {
            width: 8,
            height: 4,
            stride: 7,
            uv_offset: 64,
        },
        FrameLayout {
            width: 8,
            height: 4,
            stride: 16,
            uv_offset: 63,
        },
        FrameLayout {
            width: 8,
            height: i32::MAX - 1,
            stride: 16,
            uv_offset: 64,
        },
    ] {
        let (release, received) = mpsc::channel();
        let error = HardwareFrame::new(tempfile::tempfile().unwrap().into(), layout, 3, release)
            .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert!(received.try_recv().is_err());
    }
}

#[test]
fn hardware_frames_cannot_be_forged_or_hidden_in_host_fixtures() {
    use openpilot_cluster::models::NaviMediaFrame;
    let mut record = NaviMediaFrame::default();
    let (release, _received) = mpsc::channel();
    record.hardware_buffer = Some(
        HardwareFrame::new(
            tempfile::tempfile().unwrap().into(),
            FrameLayout {
                width: 8,
                height: 4,
                stride: 16,
                uv_offset: 64,
            },
            3,
            release,
        )
        .unwrap(),
    );
    assert!(serde_json::to_value(&record)
        .unwrap_err()
        .to_string()
        .contains("hardware frame cannot be serialized"));
    record.hardware_buffer = None;
    let mut forged = serde_json::to_value(record).unwrap();
    forged["hardware_buffer"] = serde_json::json!({"index":3,"fd":4});
    assert!(serde_json::from_value::<NaviMediaFrame>(forged).is_err());
}
