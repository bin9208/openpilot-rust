use num_traits::ToPrimitive;
// GUI sizes are bounded by allocated strings, image dimensions and finite layout constants.
#[expect(
    clippy::expect_used,
    reason = "all layout inputs are representable as f32"
)]
pub fn float(value: impl ToPrimitive) -> f32 {
    value.to_f32().expect("layout value fits f32")
}
pub fn integer(value: f32) -> Result<i32, crate::Error> {
    value
        .to_i32()
        .filter(|v| *v > 0)
        .ok_or(crate::Error::Contract(
            "render size is outside positive i32 range",
        ))
}
