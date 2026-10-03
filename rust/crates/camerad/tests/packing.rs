use openpilot_camerad::{cdm, nv12::Nv12Layout, timing::CameraEventTiming};

#[test]
fn camera_timing_limits_only_diagnostic_reporting() {
    let mut timing = CameraEventTiming::default();
    assert!(timing.observe(1_000_000_000, 1_100_000_000).report);
    for frame in 1..20 {
        assert!(
            !timing
                .observe(
                    1_000_000_000 + frame * 50_000_000,
                    1_100_000_000 + frame * 50_000_000
                )
                .report
        );
    }
    let next = timing.observe(2_000_000_000, 2_100_000_000);
    assert!(next.report);
    assert_eq!(next.suppressed, 19);
}

#[test]
fn cdm_dmi_address_is_left_zero_and_returns_the_patch_offset() {
    let mut buffer = [0xaa; 12];
    let patch = cdm::write_dmi(
        &mut buffer,
        cdm::Dmi {
            length: 256,
            address: 0xabc12345,
            selector: 7,
            opcode: 10,
        },
    )
    .unwrap();
    assert_eq!(patch, 4);
    assert_eq!(buffer, [0xff, 0, 0, 10, 0, 0, 0, 0, 0x45, 0x23, 0xc1, 7]);
}

#[test]
fn nv12_allocations_keep_page_aligned_uv_offsets() {
    for (width, height) in [(1928, 1208), (1344, 760), (2688, 1520), (1, 1)] {
        let layout = Nv12Layout::new(width, height).unwrap();
        assert_eq!(layout.stride * layout.y_height % 4096, 0);
        assert_eq!(layout.y_height / 2, layout.uv_height);
        assert_eq!(layout.size % 4096, 0);
    }
}
