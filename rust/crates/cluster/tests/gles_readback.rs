use openpilot_cluster::renderer::nv12::readback::ReadTarget;
use openpilot_startup_ui::renderer::TargetInfo;

#[test]
fn readback_target_rejects_invalid_gpu_dimensions() {
    for (framebuffer, width, height) in [(0, 4, 4), (1, 0, 4), (1, 4, -1)] {
        assert!(ReadTarget::try_from(TargetInfo {
            framebuffer,
            texture: 0,
            width,
            height
        })
        .is_err());
    }
}

#[test]
fn readback_target_preserves_raw_rgba_byte_extent() {
    let target = ReadTarget::try_from(TargetInfo {
        framebuffer: 1,
        texture: 0,
        width: 48,
        height: 64,
    })
    .unwrap();
    assert_eq!(target.byte_count(), 48 * 64 * 4);
}
