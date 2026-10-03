#![cfg(feature = "native-skip-miri")]

use openpilot_camera_kernel::Device;

#[test]
fn missing_device_reports_open_error() {
    let result = Device::open("/dev/openpilot-camera-kernel-does-not-exist");
    assert!(matches!(
        result,
        Err(openpilot_camera_kernel::Error::Open { .. })
    ));
}

#[test]
fn failed_query_returns_transport_error() {
    let device = Device::open("/dev/null").unwrap();
    let result = device.query_isp();
    assert!(matches!(
        result,
        Err(openpilot_camera_kernel::Error::Control {
            opcode: 0x101,
            code: -1,
            errno: 25
        })
    ));
}
