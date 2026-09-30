use openpilot_hardware_info::{Error, HardwareInfo, HardwarePaths, Tici};
use std::{fs, path::Path};

fn write(root: &Path, path: &str, bytes: &[u8]) {
    let path = root.join(path.trim_start_matches('/'));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

#[test]
fn serial_keeps_the_last_exact_two_field_cmdline_item() {
    // Given duplicate and over-split serial fields with a trailing newline.
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "/proc/cmdline",
        b"androidboot.serialno=first androidboot.serialno=bad=extra androidboot.serialno=last\n",
    );
    let hardware = Tici::with_paths(HardwarePaths::under(root.path()));
    // When the native identity API reads the actual fixture file.
    let serial = hardware.get_serial().unwrap();
    // Then it preserves the source's newline and duplicate-key behavior.
    assert_eq!(serial, "last\n");
}

#[test]
fn missing_serial_is_an_error_instead_of_an_invented_identity() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "/proc/cmdline", b"other=value\n");
    let hardware = Tici::with_paths(HardwarePaths::under(root.path()));
    let result = hardware.get_serial();
    assert!(matches!(result, Err(Error::Key(_))));
}

#[test]
fn successful_model_lookup_is_cached_across_instances() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "/sys/firmware/devicetree/base/model",
        b"comma tici\0",
    );
    let paths = HardwarePaths::under(root.path());
    let first = Tici::with_paths(paths.clone());
    assert_eq!(first.get_device_type().unwrap(), "tici");
    write(
        root.path(),
        "/sys/firmware/devicetree/base/model",
        b"comma mici\0",
    );
    let second = Tici::with_paths(paths);
    assert_eq!(second.get_device_type().unwrap(), "tici");
}

#[test]
fn modem_syntax_fallback_does_not_swallow_utf8_or_scalar_values() {
    let root = tempfile::tempdir().unwrap();
    let hardware = Tici::with_paths(HardwarePaths::under(root.path()));
    assert_eq!(hardware.get_modem_state().unwrap().to_json().unwrap(), "{}");
    write(root.path(), "/dev/shm/modem", b"{broken");
    assert_eq!(hardware.get_modem_state().unwrap().to_json().unwrap(), "{}");
    write(root.path(), "/dev/shm/modem", b"[1,true,null]");
    assert_eq!(
        hardware.get_modem_state().unwrap().to_json().unwrap(),
        "[1, true, null]"
    );
    assert!(matches!(hardware.get_imei(0), Err(Error::Attribute(_))));
    write(root.path(), "/dev/shm/modem", b"\xff");
    assert!(matches!(hardware.get_modem_state(), Err(Error::Utf8(_))));
}
