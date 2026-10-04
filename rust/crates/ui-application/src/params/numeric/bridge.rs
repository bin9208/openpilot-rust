#[cxx::bridge(namespace = "product_ui")]
pub(super) mod ffi {
    struct FloatResult {
        value: f32,
        valid: bool,
    }
    // SAFETY: CXX borrows the immutable slice for this call only; the parser returns scalars.
    unsafe extern "C++" {
        include!("params_numeric.h");
        fn params_float(bytes: &[u8]) -> FloatResult;
    }
}
