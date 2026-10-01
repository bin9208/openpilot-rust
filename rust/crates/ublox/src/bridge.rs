#[cxx::bridge(namespace = "ublox_serial")]
pub(crate) mod ffi {
    // SAFETY: The caller owns the live descriptor throughout this synchronous
    // call. C++ passes initialized local integers to ioctl and retains nothing.
    unsafe extern "C++" {
        include!("serial.h");
        fn raise_modem_lines(fd: i32) -> Result<()>;
    }
}
