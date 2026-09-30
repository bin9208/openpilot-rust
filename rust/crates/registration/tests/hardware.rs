use openpilot_hardware_info::{HardwarePaths, Tici};
use openpilot_registration::{Hardware, NativeHardware};
use std::fs;

#[test]
fn native_adapter_retains_raw_modem_values_and_slot_behavior() {
    // Given real, isolated identity files and the production read-only adapter.
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("proc")).unwrap();
    fs::create_dir_all(root.path().join("dev/shm")).unwrap();
    fs::write(
        root.path().join("proc/cmdline"),
        "androidboot.serialno=fixture",
    )
    .unwrap();
    fs::write(
        root.path().join("dev/shm/modem"),
        r#"{"imei":[17,null,false]}"#,
    )
    .unwrap();
    let hardware = Tici::with_paths(HardwarePaths::under(root.path()));
    let mut adapter = NativeHardware::new(&hardware);
    // When registration requests serial and both slots.
    let identity = (
        adapter.serial().unwrap(),
        adapter.imei(0).unwrap(),
        adapter.imei(1).unwrap(),
    );
    // Then no narrowing discards the JSON array and slot one stays an empty string.
    assert_eq!(identity.0, "fixture");
    assert_eq!(identity.1.to_json().unwrap(), "[17, null, false]");
    assert!(identity.2.text_eq(""));
}
