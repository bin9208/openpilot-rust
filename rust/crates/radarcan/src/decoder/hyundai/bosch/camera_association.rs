//! A0 camera/physical-object association shared by publication support checks.
use super::camera::CameraObject;
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Measurement {
    pub d_rel: f64,
    pub y_rel: f64,
    pub v_rel: f64,
}
pub fn range_span(distance: f64) -> f64 {
    1.75 + 0.12 * distance
}
pub fn long_window(span: f64, width: f64) -> (f64, f64) {
    let diff = width - 1.7;
    let extra = if 0. > diff { 0. } else { diff };
    let a = span + 4. * extra;
    let b = span + 16. * extra;
    (if 20. < a { 20. } else { a }, if 20. < b { 20. } else { b })
}
pub fn associate(
    obj: Measurement,
    cameras: &[CameraObject],
    count: usize,
) -> Result<(i32, i64, i64), &'static str> {
    let longitudinal = if 0.5 > obj.d_rel { 0.5 } else { obj.d_rel };
    let bearing = (-obj.y_rel).atan2(longitudinal);
    let span = range_span(obj.d_rel);
    let mut best = f64::INFINITY;
    let mut second = f64::INFINITY;
    let mut selected = None;
    let mut passed = 0;
    for index in 0..count {
        let cam = cameras.get(index).ok_or("list index out of range")?;
        let (l_pos, l_neg) = long_window(span, cam.width_m);
        let d_long = cam.long_m - obj.d_rel;
        let d_lat = cam.lat_m + obj.y_rel;
        let lo = if cam.angle_right < cam.angle_left {
            cam.angle_right
        } else {
            cam.angle_left
        };
        let hi = if cam.angle_right > cam.angle_left {
            cam.angle_right
        } else {
            cam.angle_left
        };
        let a = lo - bearing;
        let b = bearing - hi;
        let bear_out = if b > a { b } else { a };
        if d_long > l_pos || d_long < -l_neg || bear_out > 0.020 || d_lat.abs() > 1.75 {
            continue;
        }
        passed += 1;
        let half = (hi - lo) * 0.5;
        let denom = half + 0.020;
        let denom = if 1e-6 > denom { 1e-6 } else { denom };
        let bear_n = (half + bear_out) / denom;
        let window = l_pos + l_neg;
        let window = if 1e-6 > window { 1e-6 } else { window };
        let cost = 4. * bear_n
            + 2. * (cam.vrel_mps - obj.v_rel).abs() / 3.
            + d_lat.abs() / 1.75
            + 0.5 * d_long.abs() / window;
        if cost < best {
            second = best;
            best = cost;
            selected = Some(cam)
        } else if cost < second {
            second = cost;
        }
    }
    if passed == 0 {
        return Ok((0, -1, -1));
    }
    if passed >= 2 && second - best < 0.15 {
        return Ok((2, -1, -1));
    }
    let selected = selected.ok_or("'NoneType' object has no attribute 'episode'")?;
    Ok((1, selected.episode, selected.class_code))
}
