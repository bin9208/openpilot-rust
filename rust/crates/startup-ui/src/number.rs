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

// Coordinates may be zero or negative; unlike allocated dimensions they need no positive bound.
pub fn coordinate(value: f32) -> Result<i32, crate::Error> {
    value.to_i32().ok_or(crate::Error::Contract(
        "render coordinate is outside i32 range",
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn coordinates_accept_origin_and_negative_but_reject_nonfinite() {
        assert_eq!(super::coordinate(0.0).ok(), Some(0));
        assert_eq!(super::coordinate(-12.5).ok(), Some(-12));
        assert!(super::coordinate(f32::NAN).is_err());
        assert!(super::coordinate(f32::INFINITY).is_err());
        assert!(super::coordinate(2147483648.0).is_err());
        assert!(super::integer(0.0).is_err());
    }
}
