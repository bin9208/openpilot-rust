//! Passive presentation policy ported from carrot-wip dm_preview.py.
pub struct Input {
    pub monitoring_fresh: bool,
    pub camera_unavailable: bool,
    pub driver_fresh: bool,
    pub frame_fresh: bool,
    pub face_detected: bool,
    pub distracted: bool,
    pub lockout: bool,
    pub wheel_policy: bool,
}
pub fn sample_fresh(now: f64, timestamp: f64) -> bool {
    timestamp > 0.0 && (0.0..0.5).contains(&(now - timestamp))
}
pub fn state(input: Input) -> (&'static str, &'static str) {
    if !input.monitoring_fresh {
        ("waiting", "DM CHECK")
    } else if input.lockout {
        ("warning", "DM LIMIT")
    } else if input.camera_unavailable {
        ("wheel", "DM HANDS")
    } else if !input.driver_fresh || !input.frame_fresh {
        ("waiting", "DM CHECK")
    } else if !input.face_detected {
        ("searching", "DM FACE?")
    } else if input.distracted {
        ("warning", "DM ALERT")
    } else if input.wheel_policy {
        ("wheel", "DM HANDS")
    } else {
        ("tracking", "DM FACE")
    }
}
pub fn rect(x: f64, y: f64, width: f64, height: f64, compact: bool) -> [f64; 4] {
    if compact {
        [x + width - 154.0, y + height - 96.0, 84.0, 84.0]
    } else {
        [x + 40.0, y + 220.0, 260.0, 180.0]
    }
}
pub fn face_crop(width: f64, height: f64, position: &[f64], aspect: f64) -> Option<[f64; 4]> {
    let [fx, fy] = position else { return None };
    if !fx.is_finite() || !fy.is_finite() {
        return None;
    }
    let mirrored_x = (1080.0 - 1714.0 * fx) / 2160.0 * width;
    let tici_y = -135.0 + 504.0 + fx.abs() * 112.0 + (1205.0 - fx.abs() * 724.0) * fy;
    let center_y = height / 2.0 + (tici_y - 540.0) * width / 2160.0;
    if !(0.0..=width).contains(&mirrored_x) || !(0.0..=height).contains(&center_y) {
        return None;
    }
    let crop_h = (height * 0.42).min(width / aspect);
    let crop_w = crop_h * aspect;
    let left = (width - mirrored_x - crop_w / 2.0)
        .max(0.0)
        .min(width - crop_w);
    let top = (center_y - crop_h / 2.0).max(0.0).min(height - crop_h);
    Some([left, top, crop_w, crop_h])
}
