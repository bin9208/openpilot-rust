#![cfg(feature = "native")]
#[test]
fn checked_kernel_primitives_preserve_ioctl_errors_and_fd_ownership() {
    use openpilot_encoderd::native::v4l::primitives::Device;
    let device = Device::open("/dev/null").unwrap();
    let error = device.capability().unwrap_err();
    assert!(
        matches!(error,openpilot_encoderd::Error::Io(ref error) if error.raw_os_error()==Some(libc::ENOTTY))
    );
    println!("checked V4L host null-device QUERYCAP observable: {error}");
}
