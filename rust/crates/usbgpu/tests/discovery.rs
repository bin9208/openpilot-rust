use openpilot_usbgpu::hardware::{self, RuntimeStatus};
use std::{fs, os::unix::fs::symlink};

#[test]
fn discovery_resolves_controller_and_prioritizes_real_device_faults() {
    let root = tempfile::tempdir().unwrap();
    let controller = root.path().join("controller.ssusb");
    let usb = controller.join("usb1/1-2");
    let devices = root.path().join("devices");
    fs::create_dir_all(&usb).unwrap();
    fs::create_dir(&devices).unwrap();
    for (name, value) in [
        ("idVendor", "add1"),
        ("idProduct", "0001"),
        ("speed", "5000"),
        ("product", "custom ed4e39b7-CLEAN"),
        ("manufacturer", "fixture"),
        ("busnum", "1"),
        ("devnum", "9"),
    ] {
        fs::write(usb.join(name), value).unwrap();
    }
    fs::write(controller.join("portli"), "0x10003\n").unwrap();
    symlink(&usb, devices.join("1-2")).unwrap();
    let found = hardware::devices(&devices).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].link_error_count, 3);
    let state = RuntimeStatus {
        compiled: true,
        loading: false,
        active: true,
        startup_failed: false,
        compile_pending: false,
    };
    assert_eq!(hardware::status(&found, state), "active");
    fs::write(usb.join("speed"), "480").unwrap();
    assert_eq!(
        hardware::status(&hardware::devices(&devices).unwrap(), state),
        "slow USB (480 Mbps)"
    );
}

#[test]
fn power_bytes_preserve_signed_current_and_fault_diagnostic() {
    let power = hardware::PowerStatus::decode(&[0x40, 0x1f, 0xfe, 0xff, 1, 0, 0, 0]).unwrap();
    assert_eq!(power.voltage_mv, 8000);
    assert_eq!(power.current_ma, -2);
    assert!(power.fault);
    assert_eq!(
        hardware::power_diagnostic(Some(power)),
        Some("eGPU power fault (8000 mV, -2 mA)".into())
    );
    assert!(hardware::PowerStatus::decode(&[1, 2, 3, 4]).is_err());
}
