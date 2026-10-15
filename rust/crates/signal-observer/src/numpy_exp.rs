//! NumPy 2.5.3 Float32 exp dispatch semantics, retaining the ARM libm path.
//! Derived from loops_exponent_log.dispatch.c.src + npy_simd_data.h (BSD-3-Clause;
//! see NUMPY_LICENSE). Source sdist SHA256:
//! df2d5874ff183595a4ba404edd04f6bd9b5505c1d7708573f6a6c17489a67563.
//! This scalar FMA expression reproduces each lane of the source x86 SIMD kernel;
//! it does not reproduce NumPy's Python warning machinery or claim SIMD speed.

pub fn exp(value: f32) -> f32 {
    #[cfg(target_arch = "x86_64")]
    if std::is_x86_feature_detected!("avx512f")
        || (std::is_x86_feature_detected!("avx2") && std::is_x86_feature_detected!("fma"))
    {
        return simd_exp(value);
    }
    value.exp()
}

#[cfg(target_arch = "x86_64")]
fn simd_exp(x: f32) -> f32 {
    if x.is_nan() {
        return f32::from_bits(0x7fc00000);
    }
    if x >= 88.72284 {
        return f32::INFINITY;
    }
    if x <= -103.972084 {
        return 0.;
    }
    let q = (x * f32::from_bits(0x3fb8aa3b) + 12582912.) - 12582912.;
    let y = q.mul_add(f32::from_bits(0xbf317200), x);
    let y = q.mul_add(f32::from_bits(0xb5bfbe8e), y);
    let y = q.mul_add(0., y);
    let p = f32::from_bits(0x3a053dd8).mul_add(y, f32::from_bits(0x3bdd7159));
    let p = p.mul_add(y, f32::from_bits(0x3d517d8c));
    let p = p.mul_add(y, f32::from_bits(0x3e7d4c58));
    let p = p.mul_add(y, f32::from_bits(0x3f39cbd5));
    let p = p.mul_add(y, 1.);
    let d = f32::from_bits(0x3cb0e832).mul_add(y, f32::from_bits(0xbe8c6857));
    let d = d.mul_add(y, 1.);
    (f64::from(p / d) * 2_f64.powi(q as i32)) as f32
}
