use super::math::byte;
use crate::{paint::color, Error};
pub const PATH: [u32; 10] = [
    color(255, 0, 0, 120),
    color(255, 153, 0, 120),
    color(218, 202, 37, 120),
    color(0, 153, 0, 120),
    color(0, 0, 255, 120),
    color(0, 0, 128, 120),
    color(139, 0, 255, 120),
    color(218, 111, 37, 120),
    color(255, 255, 255, 120),
    color(0, 0, 0, 120),
];
pub const THROTTLE: [u32; 3] = [
    color(13, 248, 122, 102),
    color(114, 255, 92, 89),
    color(114, 255, 92, 0),
];
pub const COAST: [u32; 3] = [
    color(242, 242, 242, 102),
    color(242, 242, 242, 89),
    color(242, 242, 242, 0),
];
pub fn path(index: i32) -> Result<u32, Error> {
    let index =
        usize::try_from(index.rem_euclid(10)).map_err(|_| Error::Contract("path palette index"))?;
    Ok(PATH[index])
}
pub fn blend(a: u32, b: u32, t: f64) -> Result<u32, Error> {
    if t >= 1. {
        return Ok(b);
    }
    if t <= 0. {
        return Ok(a);
    }
    let (a, b) = (a.to_le_bytes(), b.to_le_bytes());
    let mut c = [0; 4];
    for i in 0..4 {
        c[i] = byte((1. - t) * f64::from(a[i]) + t * f64::from(b[i]))?;
    }
    Ok(u32::from_le_bytes(c))
}
pub fn hls(h: f64, l: f64, s: f64, a: f64) -> Result<u32, Error> {
    let rgb = if s == 0. {
        [l; 3]
    } else {
        let m2 = if l <= 0.5 {
            l * (1. + s)
        } else {
            l + s - l * s
        };
        let m1 = 2. * l - m2;
        let value = |h: f64| {
            let h = h.rem_euclid(1.);
            if h < 1. / 6. {
                m1 + (m2 - m1) * h * 6.
            } else if h < 0.5 {
                m2
            } else if h < 2. / 3. {
                m1 + (m2 - m1) * (2. / 3. - h) * 6.
            } else {
                m1
            }
        };
        [value(h + 1. / 3.), value(h), value(h - 1. / 3.)]
    };
    Ok(color(
        byte(rgb[0] * 255.)?,
        byte(rgb[1] * 255.)?,
        byte(rgb[2] * 255.)?,
        byte(a * 255.)?,
    ))
}
