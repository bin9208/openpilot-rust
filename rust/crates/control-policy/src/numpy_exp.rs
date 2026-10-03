// NumPy 2.5.3 loops_exponent_log.dispatch.c.src and npy_simd_data.h;
// preserve its Float32 FMA/range-reduction order. See NUMPY-LICENSE.txt.
pub fn exp(value: f32) -> f32 {
    #[cfg(target_arch = "x86_64")]
    if std::is_x86_feature_detected!("avx2") && std::is_x86_feature_detected!("fma") {
        return polynomial(value);
    }
    value.exp()
}

#[expect(
    clippy::excessive_precision,
    reason = "retain the NumPy 2.5.3 source coefficients verbatim before f32 rounding"
)]
pub fn polynomial(value: f32) -> f32 {
    if value.is_nan() {
        return f32::NAN;
    }
    if value >= 88.72284_f32 {
        return f32::INFINITY;
    }
    if value <= -103.972084_f32 {
        return 0.;
    }
    let mut quadrant = value * std::f32::consts::LOG2_E;
    quadrant += 12582912.;
    quadrant -= 12582912.;
    let x = quadrant.mul_add(-0.693145752_f32, value);
    let x = quadrant.mul_add(-1.42860677e-6_f32, x);
    let p = 5.082762527590693718096e-4_f32.mul_add(x, 6.757896990527504603057e-3_f32);
    let p = p.mul_add(x, 5.114512081637298353406e-2_f32);
    let p = p.mul_add(x, 2.473615434895520810817e-1_f32);
    let p = p.mul_add(x, 7.257664613233124478488e-1_f32);
    let p = p.mul_add(x, 9.999999999980870924916e-1_f32);
    let q = 2.159509375685829852307e-2_f32.mul_add(x, -2.742335390411667452936e-1_f32);
    let q = q.mul_add(x, 1.);
    let poly = p / q;
    let exponent = quadrant as i32;
    if exponent <= -125 {
        let base = f32::from_bits(poly.to_bits().wrapping_add((-125_i32 as u32) << 23));
        base / ((1_u32 << (-125 - exponent)) as f32)
    } else {
        f32::from_bits(poly.to_bits().wrapping_add((exponent as u32) << 23))
    }
}
