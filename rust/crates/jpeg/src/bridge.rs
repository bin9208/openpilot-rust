#[cxx::bridge(namespace = "openpilot_jpeg")]
pub(crate) mod ffi {
    // SAFETY: the adapter validates dimensions against the borrowed RGB slice,
    // retains no input pointer, and returns a new owned Vec. JPEG longjmp stays in C.
    unsafe extern "C++" {
        include!("bridge.h");
        fn encode(rgb: &[u8], width: u32, height: u32) -> Result<Vec<u8>>;
    }
}
