pub fn command(binary: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    if let Some(qemu) = std::env::var_os("IPC194_TEST_QEMU") {
        let sysroot = std::env::var_os("IPC194_TEST_SYSROOT")
            .expect("QEMU test execution requires a sysroot");
        let mut command = std::process::Command::new(qemu);
        command.arg("-L").arg(sysroot).arg(binary);
        command
    } else {
        std::process::Command::new(binary)
    }
}
