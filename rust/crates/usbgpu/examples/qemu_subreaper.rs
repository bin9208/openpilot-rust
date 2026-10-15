//! Host-only test launcher for QEMU versions that do not emulate subreaper prctl.
//! Exec preserves the verified host kernel attribute; all child cleanup remains ARM code.
#[cfg(target_os = "linux")]
fn main() -> std::io::Result<()> {
    use std::{
        ffi::{c_int, c_ulong},
        os::unix::process::CommandExt,
        process::Command,
    };
    unsafe extern "C" {
        fn prctl(option: c_int, ...) -> c_int;
    }
    // SAFETY: Linux prctl PR_SET_CHILD_SUBREAPER accepts an integer boolean;
    // all unused variadic arguments are zero of the ABI's unsigned-long width.
    if unsafe { prctl(36, 1 as c_ulong, 0 as c_ulong, 0 as c_ulong, 0 as c_ulong) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut verified: c_int = 0;
    // SAFETY: PR_GET_CHILD_SUBREAPER writes one c_int to this live exclusive stack slot.
    if unsafe {
        prctl(
            37,
            &mut verified as *mut c_int,
            0 as c_ulong,
            0 as c_ulong,
            0 as c_ulong,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error());
    }
    if verified != 1 {
        return Err(std::io::Error::other("host subreaper attribute not set"));
    }
    let mut args = std::env::args_os().skip(1);
    let program = args
        .next()
        .ok_or_else(|| std::io::Error::other("expected QEMU executable and ARM test arguments"))?;
    eprintln!("native host subreaper verified; exec preserves it for the ARM cleanup test");
    Err(Command::new(program)
        .args(args)
        .env("OPENPILOT_TEST_SUBREAPER_INHERITED", "1")
        .exec())
}
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("subreaper test launcher requires Linux");
    std::process::exit(1);
}
