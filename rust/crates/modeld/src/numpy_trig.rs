// NumPy 2.4.6 float32 trig, b832a09cf2a169c833dd2371e7c07aa00b293242.
// Copyright (c) 2005-2025, NumPy Developers. See NUMPY-LICENSE.txt.
pub(crate) fn trig(value: f32, cosine: bool) -> f32 {
    #[cfg(target_arch = "x86_64")]
    if (std::is_x86_feature_detected!("avx2") && std::is_x86_feature_detected!("fma"))
        || std::is_x86_feature_detected!("avx512f")
    {
        return polynomial(value, cosine);
    }
    if cosine {
        value.cos()
    } else {
        value.sin()
    }
}

#[cfg(target_arch = "x86_64")]
fn polynomial(value: f32, cosine: bool) -> f32 {
    if value.is_nan() {
        return value;
    }
    if value.abs() > f32::from_bits(if cosine { 0x478b_9a08 } else { 0x47e5_5dff }) {
        return if cosine { value.cos() } else { value.sin() };
    }
    let quadrant = value.mul_add(f32::from_bits(0x3f22_f983), 12582912.0) - 12582912.0;
    let reduced = quadrant.mul_add(f32::from_bits(0xbfc9_0fd8), value);
    let reduced = quadrant.mul_add(f32::from_bits(0xb4a8_885a), reduced);
    let reduced = quadrant.mul_add(f32::from_bits(0xa7c2_34c4), reduced);
    let square = reduced * reduced;
    let selection = quadrant as i32 + i32::from(cosine);
    let result = if selection & 1 == 0 {
        let result = f32::from_bits(0x363e_9dde).mul_add(square, f32::from_bits(0xb950_35dd));
        let result = result.mul_add(square, f32::from_bits(0x3c08_88cd));
        let result = result.mul_add(square, f32::from_bits(0xbe2a_aaab));
        result.mul_add(square, 0.0).mul_add(reduced, reduced)
    } else {
        let result = f32::from_bits(0x37cc_730b).mul_add(square, f32::from_bits(0xbab6_036e));
        let result = result.mul_add(square, f32::from_bits(0x3d2a_aa9e));
        let result = result.mul_add(square, -0.5);
        result.mul_add(square, 1.0)
    };
    if selection & 2 != 0 {
        0.0 - result
    } else {
        result
    }
}
