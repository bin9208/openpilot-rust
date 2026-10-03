#![cfg(feature = "native-skip-miri")]
use openpilot_panda_spi::linux_io::OptionKind;
use openpilot_panda_spi_linux::FirmwareKernel;

#[test]
fn firmware_syscalls_reject_owned_regular_file_and_close_exactly_once() {
    let path = std::env::temp_dir().join(format!("panda-firmware-kernel-{}", std::process::id()));
    drop(
        std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .unwrap(),
    );
    let before = std::fs::read_dir("/proc/self/fd").unwrap().count();
    for _ in 0..32 {
        let mut kernel = FirmwareKernel::at_path(path.to_str().unwrap());
        assert!(kernel.exists());
        kernel.open().unwrap();
        assert_eq!(
            kernel
                .read_option(OptionKind::Mode)
                .unwrap_err()
                .raw_os_error(),
            Some(25)
        );
        assert_eq!(
            kernel.set_speed(1_000_000).unwrap_err().raw_os_error(),
            Some(25)
        );
        let mut reply = [0xa5; 8];
        assert_eq!(
            kernel
                .transfer(&[0; 7], &mut reply, 50_000_000, 8)
                .unwrap_err()
                .raw_os_error(),
            Some(22)
        );
        assert_eq!(
            kernel
                .transfer(&[0; 8], &mut reply, 50_000_000, 8)
                .unwrap_err()
                .raw_os_error(),
            Some(25)
        );
        assert_eq!(
            kernel
                .kernel_transfer(0, &[1, 2], &mut reply, false)
                .unwrap_err()
                .raw_os_error(),
            Some(25)
        );
        assert_eq!(reply, [0xa5; 8]);
        kernel.flock(true).unwrap();
        kernel.flock(false).unwrap();
        kernel.close();
        kernel.close();
    }
    assert_eq!(std::fs::read_dir("/proc/self/fd").unwrap().count(), before);
    std::fs::remove_file(path).unwrap();
}
