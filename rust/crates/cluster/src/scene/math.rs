use super::{Color, Vec3};

#[must_use]
pub fn clamp(value: f64, low: f64, high: f64) -> f64 {
    low.max(high.min(value))
}

#[must_use]
pub fn smoothstep(value: f64) -> f64 {
    let value = clamp(value, 0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

#[must_use]
pub fn rgba(color: [u8; 3], alpha: u8) -> Color {
    [color[0], color[1], color[2], alpha]
}

#[must_use]
pub fn rgba_with_alpha(color: [u8; 3], alpha: f64) -> Color {
    rgba(
        color,
        num_traits::ToPrimitive::to_u8(&clamp(alpha, 0.0, 255.0)).unwrap_or(0),
    )
}

#[must_use]
pub fn blend_color(color: [u8; 3], target: [u8; 3], amount: f64) -> [u8; 3] {
    let amount = clamp(amount, 0.0, 1.0);
    std::array::from_fn(|index| {
        let value =
            f64::from(color[index]) + (f64::from(target[index]) - f64::from(color[index])) * amount;
        num_traits::ToPrimitive::to_u8(&value.round_ties_even()).unwrap_or(0)
    })
}

#[must_use]
pub fn lighten(color: [u8; 3], amount: f64) -> [u8; 3] {
    blend_color(color, [255; 3], amount)
}

#[must_use]
pub fn darken(color: [u8; 3], amount: f64) -> [u8; 3] {
    blend_color(color, [0; 3], amount)
}

#[must_use]
pub fn normalize2(x: f64, y: f64) -> (f64, f64) {
    let length = x.hypot(y);
    if length <= 0.0001 {
        (0.0, 1.0)
    } else {
        (x / length, y / length)
    }
}

#[must_use]
pub fn sample_range(start: f64, end: f64, steps: u32) -> Vec<f64> {
    let steps = steps.max(1);
    (0..=steps)
        .map(|index| start + (end - start) * f64::from(index) / f64::from(steps))
        .collect()
}

#[must_use]
pub fn lerp_vec3(start: Vec3, end: Vec3, amount: f64) -> Vec3 {
    Vec3::new(
        start.x + (end.x - start.x) * amount,
        start.y + (end.y - start.y) * amount,
        start.z + (end.z - start.z) * amount,
    )
}

pub fn append_unique_point(points: &mut Vec<Vec3>, point: Vec3) {
    if points.last() != Some(&point) {
        points.push(point);
    }
}

#[must_use]
pub fn vec3_with_x_offset(point: Vec3, offset: f64) -> Vec3 {
    Vec3::new(point.x + offset, point.y, point.z)
}
