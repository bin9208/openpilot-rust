use openpilot_driving_modeld::usb_selection::{refresh, Configuration};
use openpilot_params::Params;
use openpilot_usbgpu::model::Paths;
use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};
fn config(root: &Path) -> Configuration {
    Configuration {
        paths: Paths {
            models: root.join("models"),
            cache: root.join("cache"),
            assets: root.join("assets"),
        },
        devices: root.join("devices"),
        worker: root.join("unused-worker"),
        cancelled: Arc::new(AtomicBool::new(false)),
    }
}
#[test]
fn inactive_usb_with_unchanged_persisted_values_performs_no_writes() {
    let temp = tempfile::tempdir().unwrap();
    let params = Params::open(temp.path(), "test").unwrap();
    let config = config(temp.path());
    refresh(&config, &params).unwrap();
    let old = fs::File::open(params.directory().join("UsbGpuPresent")).unwrap();
    let old_compiled = fs::File::open(params.directory().join("UsbGpuCompiled")).unwrap();
    fs::remove_file(temp.path().join(".lock")).unwrap();
    fs::create_dir(temp.path().join(".lock")).unwrap();
    refresh(&config, &params).unwrap();
    assert_eq!(
        fs::metadata(params.directory().join("UsbGpuPresent"))
            .unwrap()
            .ino(),
        old.metadata().unwrap().ino()
    );
    assert_eq!(
        fs::metadata(params.directory().join("UsbGpuCompiled"))
            .unwrap()
            .ino(),
        old_compiled.metadata().unwrap().ino()
    );
}
#[test]
fn external_edits_and_deleted_values_are_restored_without_clearing_seen_history() {
    let temp = tempfile::tempdir().unwrap();
    let params = Params::open(temp.path(), "test").unwrap();
    let config = config(temp.path());
    refresh(&config, &params).unwrap();
    params.put_bool("UsbGpuHardwareSeen", true).unwrap();
    params.put_bool("UsbGpuPresent", true).unwrap();
    params.put_bool("UsbGpuCompiled", true).unwrap();
    refresh(&config, &params).unwrap();
    assert_eq!(
        params.get("UsbGpuPresent").unwrap().as_deref(),
        Some(b"0".as_slice())
    );
    assert!(!params.get_bool("UsbGpuCompiled").unwrap());
    assert!(params.get_bool("UsbGpuHardwareSeen").unwrap());
    fs::remove_file(params.directory().join("UsbGpuPresent")).unwrap();
    fs::write(params.directory().join("UsbGpuCompiled"), b"0\n").unwrap();
    refresh(&config, &params).unwrap();
    assert_eq!(
        params.get("UsbGpuPresent").unwrap().as_deref(),
        Some(b"0".as_slice())
    );
    assert_eq!(
        params.get("UsbGpuCompiled").unwrap().as_deref(),
        Some(b"0".as_slice())
    );
}
#[test]
fn present_device_records_seen_and_retains_it_after_removal() {
    let temp = tempfile::tempdir().unwrap();
    let params = Params::open(temp.path(), "test").unwrap();
    let config = config(temp.path());
    let device = config.devices.join("usb-test");
    fs::create_dir_all(&device).unwrap();
    for (name, value) in [
        ("idVendor", "add1"),
        ("idProduct", "0001"),
        ("speed", "5000"),
    ] {
        fs::write(device.join(name), value).unwrap();
    }
    refresh(&config, &params).unwrap();
    assert!(params.get_bool("UsbGpuPresent").unwrap());
    assert!(params.get_bool("UsbGpuHardwareSeen").unwrap());
    params.put_bool("UsbGpuHardwareSeen", false).unwrap();
    refresh(&config, &params).unwrap();
    assert!(params.get_bool("UsbGpuHardwareSeen").unwrap());
    fs::remove_file(params.directory().join("UsbGpuHardwareSeen")).unwrap();
    refresh(&config, &params).unwrap();
    assert!(params.get_bool("UsbGpuHardwareSeen").unwrap());
    // Repeated presence must also skip the durable HardwareSeen write.
    fs::remove_file(temp.path().join(".lock")).unwrap();
    fs::create_dir(temp.path().join(".lock")).unwrap();
    refresh(&config, &params).unwrap();
    fs::remove_dir(temp.path().join(".lock")).unwrap();
    fs::remove_dir_all(device).unwrap();
    refresh(&config, &params).unwrap();
    assert!(!params.get_bool("UsbGpuPresent").unwrap());
    assert!(params.get_bool("UsbGpuHardwareSeen").unwrap());
}

#[test]
fn refresh_preserves_activity_loading_and_startup_failure_flags() {
    let temp = tempfile::tempdir().unwrap();
    let params = Params::open(temp.path(), "test").unwrap();
    let config = config(temp.path());
    for value in [true, false] {
        for key in ["UsbGpuActive", "UsbGpuLoading", "UsbGpuStartupFailed"] {
            params.put_bool(key, value).unwrap();
        }
        refresh(&config, &params).unwrap();
        for key in ["UsbGpuActive", "UsbGpuLoading", "UsbGpuStartupFailed"] {
            assert_eq!(params.get_bool(key).unwrap(), value, "{key}");
        }
    }
}
#[test]
fn read_and_write_failures_propagate_and_retry_against_current_persisted_state() {
    let temp = tempfile::tempdir().unwrap();
    let params = Params::open(temp.path(), "test").unwrap();
    let config = config(temp.path());
    refresh(&config, &params).unwrap();
    params.put_bool("UsbGpuPresent", true).unwrap();
    fs::remove_file(temp.path().join(".lock")).unwrap();
    fs::create_dir(temp.path().join(".lock")).unwrap();
    assert!(refresh(&config, &params).is_err());
    assert!(params.get_bool("UsbGpuPresent").unwrap());
    fs::remove_dir(temp.path().join(".lock")).unwrap();
    refresh(&config, &params).unwrap();
    assert!(!params.get_bool("UsbGpuPresent").unwrap());
    fs::remove_file(params.directory().join("UsbGpuPresent")).unwrap();
    fs::create_dir(params.directory().join("UsbGpuPresent")).unwrap();
    assert!(refresh(&config, &params).is_err());
    fs::remove_dir(params.directory().join("UsbGpuPresent")).unwrap();
    refresh(&config, &params).unwrap();
    assert!(!params.get_bool("UsbGpuPresent").unwrap());
}
