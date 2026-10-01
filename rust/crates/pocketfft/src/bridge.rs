#[cxx::bridge(namespace = "openpilot_pocketfft")]
pub(crate) mod ffi {
    #[derive(Clone, Copy, Default)]
    struct Complex {
        re: f64,
        im: f64,
    }
    // SAFETY: CXX owns Plan, bounds Slice, pins mutable access and catches C++ exceptions.
    // The adapter copies initialized scalar fields; it retains no Rust references or pointers.
    unsafe extern "C++" {
        include!("bridge.h");
        type Plan;
        fn plan(size: usize) -> Result<UniquePtr<Plan>>;
        fn transform(
            self: Pin<&mut Plan>,
            data: &mut [Complex],
            scale: f64,
            forward: bool,
        ) -> Result<()>;
    }
}
