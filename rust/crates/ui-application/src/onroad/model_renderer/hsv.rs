//! Float32 raylib ColorToHSV/ColorFromHSV arithmetic used by mici/onroad.blend_colors.
//! Raylib provenance and zlib license remain in third_party/raylib/include/raylib.h.
use super::math::{byte, float};
use crate::Error;
fn hsv(color: u32) -> [f32; 3] {
    let [r, g, b, _] = color.to_le_bytes().map(f32::from);
    let (r, g, b) = (r / 255., g / 255., b / 255.);
    let low = r.min(g).min(b);
    let high = r.max(g).max(b);
    let delta = high - low;
    if delta < 0.00001 {
        return [0., 0., high];
    }
    let saturation = delta / high;
    let mut hue = if r >= high {
        (g - b) / delta
    } else if g >= high {
        2. + (b - r) / delta
    } else {
        4. + (r - g) / delta
    };
    hue *= 60.;
    if hue < 0. {
        hue += 360.;
    }
    [hue, saturation, high]
}
pub fn blend(a: u32, b: u32, fraction: f64) -> Result<u32, Error> {
    let [h0, s0, v0] = hsv(a).map(f64::from);
    let [h1, s1, v1] = hsv(b).map(f64::from);
    let delta = (h1 - h0 + 180.).rem_euclid(360.) - 180.;
    let hue = float((h0 + fraction * delta).rem_euclid(360.));
    let saturation = float(s0 + fraction * (s1 - s0));
    let value = float(v0 + fraction * (v1 - v0));
    let mut channels = [0, 0, 0, 255];
    for (i, offset) in [5_f32, 3., 1.].into_iter().enumerate() {
        let k = (offset + hue / 60.) % 6.;
        let k = (4. - k).min(k).clamp(0., 1.);
        channels[i] = byte(f64::from((value - value * saturation * k) * 255.))?;
    }
    Ok(u32::from_le_bytes(channels))
}
