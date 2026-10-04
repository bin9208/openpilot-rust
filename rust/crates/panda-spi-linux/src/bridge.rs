#[cxx::bridge(namespace = "panda_spi_kernel")]
pub(crate) mod ffi {
    struct Call {
        result: i32,
        error_number: i32,
        fd: i32,
        request: u64,
        argument_address: u64,
    }
    struct OptionCall {
        call: Call,
        value: u32,
    }
    // SAFETY: category 8 FFI. C++ owns the descriptor in a UniquePtr and closes it
    // once. Calls are synchronous, retain no borrows, bound ioctl lengths to both
    // slices and initialize the entire Linux UAPI struct. Pin<&mut> serializes access.
    unsafe extern "C++" {
        include!("kernel.h");
        type Handle;
        fn create(path: &CxxString) -> UniquePtr<Handle>;
        fn exists(self: &Handle) -> bool;
        fn open(self: Pin<&mut Handle>) -> i32;
        fn open_call(self: Pin<&mut Handle>) -> Call;
        fn read_option(self: Pin<&mut Handle>, option: u8) -> OptionCall;
        fn configure(self: Pin<&mut Handle>, option: u8, value: u32) -> Call;
        fn transfer(self: Pin<&mut Handle>, tx: &[u8], rx: &mut [u8]) -> Call;
        fn transfer_at_speed(
            self: Pin<&mut Handle>,
            tx: &[u8],
            rx: &mut [u8],
            speed: u32,
            bits: u8,
        ) -> Call;
        fn read_bytes(self: Pin<&mut Handle>, rx: &mut [u8]) -> Call;
        fn write_bytes(self: Pin<&mut Handle>, tx: &[u8]) -> Call;
        fn firmware_transfer(
            self: Pin<&mut Handle>,
            endpoint: u8,
            tx: &[u8],
            rx: &mut [u8],
            disconnect: bool,
        ) -> Call;
        fn flock_call(self: Pin<&mut Handle>, exclusive: bool) -> Call;
        fn flock(self: Pin<&mut Handle>, exclusive: bool);
        fn close(self: Pin<&mut Handle>);
        fn now_ns() -> u64;
        fn yield_now();
        fn sleep_us(micros: u32);
        fn set_scheduler(policy: i32, priority: i32) -> Call;
        fn errno_description(error: i32) -> UniquePtr<CxxString>;
        fn parse_probability(value: &CxxString) -> Result<f64>;
        fn random() -> u32;
        fn diagnostic_print(text: &str);
    }
}
