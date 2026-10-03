use openpilot_camerad::ioctl::{retry_interrupted, ControlEnvelope, SyscallResult};

#[test]
fn bps_camera_envelope_ignores_embedded_sync_error() {
    let mut camera = ControlEnvelope::camera(0, 0x1122, 68);
    let mut sync = ControlEnvelope::sync(0, 0x1122, 68);
    assert_eq!(&camera.bytes()[8..12], &1_u32.to_le_bytes());
    assert_eq!(&sync.bytes()[8..12], &[0; 4]);
    camera.set_kernel_result((-5_i32) as u32);
    sync.set_kernel_result((-5_i32) as u32);
    assert_eq!(camera.effective_result(0), 0);
    assert_eq!(sync.effective_result(0), -5);
    assert_eq!(sync.effective_result(-1), -1);
}

#[test]
fn memory_handle_probe_uses_size_eight_and_type_two() {
    let probe = ControlEnvelope::camera(0x10a, 0xf123_4567, 0);
    assert_eq!(&probe.bytes()[4..12], &[8, 0, 0, 0, 2, 0, 0, 0]);
    assert_eq!(&probe.bytes()[16..24], &0xf123_4567_u64.to_le_bytes());
}

#[test]
fn interrupted_ioctl_has_one_hundred_retries_and_eagain_has_none() {
    let mut attempts = 0;
    let result = retry_interrupted(|| {
        attempts += 1;
        SyscallResult { code: -1, errno: 4 }
    });
    assert_eq!(result.code, -1);
    assert_eq!(attempts, 101);
    attempts = 0;
    retry_interrupted(|| {
        attempts += 1;
        SyscallResult {
            code: -1,
            errno: 11,
        }
    });
    assert_eq!(attempts, 1);
}
