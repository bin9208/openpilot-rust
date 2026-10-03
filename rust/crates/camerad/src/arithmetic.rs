// SConstruct's larch64 Clang/cortex-a57 build contracts operations within an
// expression. The default x86_64 reference build does not enable FMA.
pub(crate) fn multiply_add32(left: f32, right: f32, addend: f32) -> f32 {
    if cfg!(target_arch = "aarch64") {
        left.mul_add(right, addend)
    } else {
        left * right + addend
    }
}

pub(crate) fn multiply_add64(left: f64, right: f64, addend: f64) -> f64 {
    if cfg!(target_arch = "aarch64") {
        left.mul_add(right, addend)
    } else {
        left * right + addend
    }
}
