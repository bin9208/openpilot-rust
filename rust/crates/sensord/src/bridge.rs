#[cxx::bridge(namespace = "sensord_kernel")]
pub(crate) mod ffi {
    // SAFETY: FFI category 8. Linux arguments are value-initialized, lengths are
    // bounded before copying, and C++ owns returned vectors/descriptors through
    // UniquePtr. No borrowed string or descriptor pointer is retained by C++.
    unsafe extern "C++" {
        include!("kernel.h");
        fn read_byte(fd: i32, address: u16, reg: u8, force: bool) -> Result<u8>;
        fn write_byte(fd: i32, address: u16, reg: u8, value: u8, force: bool) -> Result<()>;
        fn read_block(
            fd: i32,
            address: u16,
            reg: u8,
            length: usize,
            force: bool,
        ) -> Result<UniquePtr<CxxVector<u8>>>;
        fn realtime(pc: bool) -> Result<()>;
        type Gpio;
        fn open_gpio(path: &CxxString, label: &CxxString, pin: u32) -> Result<UniquePtr<Gpio>>;
        fn poll_event(self: Pin<&mut Gpio>, timeout_ms: i32) -> Result<i32>;
        fn read_events(self: Pin<&mut Gpio>) -> Result<UniquePtr<CxxVector<u8>>>;
    }
}
