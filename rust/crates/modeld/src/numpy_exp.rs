// NumPy 2.4.6 float32 SIMD exponential, ported from b832a09cf2a169c833dd2371e7c07aa00b293242.
// Copyright (c) 2005-2025, NumPy Developers. See NUMPY-LICENSE.txt.
// Preserve x86 polynomial rounding because it can change MDN and FCW decisions.
pub(crate) fn exp(value: f32) -> f32 {
    #[cfg(target_arch = "x86_64")]
    if (std::is_x86_feature_detected!("avx2") && std::is_x86_feature_detected!("fma"))
        || std::is_x86_feature_detected!("avx512f")
    {
        return simd_polynomial(value);
    }
    value.exp()
}

#[cfg(target_arch = "x86_64")]
fn simd_polynomial(value: f32) -> f32 {
    if value.is_nan() {
        return value;
    }
    if value >= f32::from_bits(0x42b1_7218) {
        return f32::INFINITY;
    }
    if value <= f32::from_bits(0xc2cf_f1b5) {
        return 0.0;
    }
    let quadrant = ((value * f32::from_bits(0x3fb8_aa3b)) + 12582912.0) - 12582912.0;
    let reduced = quadrant.mul_add(f32::from_bits(0xbf31_7200), value);
    let reduced = quadrant.mul_add(f32::from_bits(0xb5bf_be8e), reduced);
    let reduced = quadrant.mul_add(0.0, reduced);
    let numerator = f32::from_bits(0x3a05_3dd8).mul_add(reduced, f32::from_bits(0x3bdd_7159));
    let numerator = numerator.mul_add(reduced, f32::from_bits(0x3d51_7d8c));
    let numerator = numerator.mul_add(reduced, f32::from_bits(0x3e7d_4c58));
    let numerator = numerator.mul_add(reduced, f32::from_bits(0x3f39_cbd5));
    let numerator = numerator.mul_add(reduced, 1.0);
    let denominator = f32::from_bits(0x3cb0_e832).mul_add(reduced, f32::from_bits(0xbe8c_6857));
    let denominator = denominator.mul_add(reduced, 1.0);
    let polynomial = numerator / denominator;
    // The finite range checks bound the integral quadrant to [-150, 128].
    let exponent = quadrant as i32;
    let normal = u32::from_ne_bytes(exponent.max(-125).to_ne_bytes());
    let scaled = f32::from_bits(polynomial.to_bits().wrapping_add(normal << 23));
    if exponent <= -125 {
        scaled / 2.0_f32.powi(-exponent - 125)
    } else {
        scaled
    }
}
