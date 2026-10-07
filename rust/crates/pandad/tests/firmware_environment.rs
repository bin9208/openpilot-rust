#![cfg(feature = "native-skip-miri")]
use openpilot_pandad::firmware::{
    client::Environment, native_environment::NativeEnvironment, native_spi::Pool,
};

#[test]
fn firmware_signature_requires_a_complete_tail_and_regular_file() {
    let directory =
        std::env::temp_dir().join(format!("panda-firmware-files-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let mut environment = NativeEnvironment::new(
        openpilot_panda_usb::Api::system().unwrap(),
        Pool::at_path("/unused-owned-test-spi"),
        directory.clone(),
        |_, _, _| Ok(()),
    );
    let path = directory.join("panda.bin.signed");
    assert!(!environment.file_exists(&directory).unwrap());
    assert!(!environment.file_exists(&path).unwrap());
    std::fs::write(&path, [7; 127]).unwrap();
    assert!(environment.file_exists(&path).unwrap());
    assert!(environment.file_read(&path, Some(128)).is_err());
    let data: Vec<_> = (0..255).collect();
    std::fs::write(&path, &data).unwrap();
    assert_eq!(
        environment.file_read(&path, Some(128)).unwrap(),
        data[127..]
    );
    assert_eq!(environment.file_read(&path, None).unwrap(), data);
    std::fs::remove_dir_all(directory).unwrap();
}
