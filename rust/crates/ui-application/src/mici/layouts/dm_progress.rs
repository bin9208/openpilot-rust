//! Progress arithmetic from TrainingGuideDMTutorial in mici/layouts/onboarding.py.
use crate::paint;
use num_traits::ToPrimitive;
use openpilot_ui_framework::{
    draw::Ring,
    geometry::{Point, Rect},
    text_layout::float,
};
#[derive(Default, Debug, serde::Serialize)]
pub struct Progress {
    pub value: f64,
    pub good_enabled: bool,
}
#[derive(serde::Deserialize)]
pub struct Input {
    pub received: bool,
    pub face_detected: bool,
    pub orientation: Vec<f64>,
    pub bad_face_page: bool,
    pub fps: f64,
}
impl Progress {
    pub fn show(&mut self) {
        self.value = 0.0;
    }
    pub fn update(&mut self, input: &Input) {
        if !input.received {
            return;
        }
        let center = match input.orientation.as_slice() {
            [pitch, yaw, _] => pitch.to_degrees().abs() < 30.0 && yaw.to_degrees().abs() < 30.0,
            _ => false,
        };
        if ((input.face_detected && center) || self.value > 0.99) && !input.bad_face_page {
            let duration = if self.value < 0.25 { 8.0 } else { 4.0 };
            self.value = (self.value + 1.0 / (duration * input.fps)).min(1.0);
        } else {
            let dt = 1.0 / input.fps;
            let alpha = dt / (0.5 + dt);
            self.value *= 1.0 - alpha;
        }
        self.good_enabled = self.value >= 0.999;
    }
    pub fn ring(&self, rect: Rect, right_hand_drive: bool) -> Ring {
        let end = 90.0 + self.value * 360.0;
        let angle = end - 90.0;
        let alpha = interpolate(angle, 0.0, 45.0, 0.0, 255.0);
        let color_t = interpolate(angle, 45.0, 360.0, 0.0, 1.0).clamp(0.0, 1.0);
        let r = interpolate(color_t, 0.0, 1.0, 255.0, 0.0);
        let b = interpolate(color_t, 0.0, 1.0, 255.0, 64.0);
        Ring {
            center: Point {
                x: if right_hand_drive {
                    float(f64::from(rect.x) + 68.0)
                } else {
                    float(f64::from(rect.x) + f64::from(rect.width) - 68.0)
                },
                y: float(f64::from(rect.y) + 68.0),
            },
            inner: 53.0,
            outer: 61.0,
            start: 90.0,
            end: float(end),
            segments: 36,
            color: paint::color(
                r.to_u8().unwrap_or(0),
                255,
                b.to_u8().unwrap_or(0),
                alpha.to_u8().unwrap_or(0),
            ),
        }
    }
}
fn interpolate(x: f64, left: f64, right: f64, start: f64, end: f64) -> f64 {
    if x <= left {
        start
    } else if x >= right {
        end
    } else {
        (end - start) / (right - left) * (x - left) + start
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> Input {
        Input {
            received: true,
            face_detected: true,
            orientation: vec![0.0; 3],
            bad_face_page: false,
            fps: 20.0,
        }
    }
    #[test]
    fn threshold_sticky_and_bad_face_decay() {
        let mut p = Progress::default();
        let mut i = input();
        for _ in 0..120 {
            p.update(&i);
        }
        assert!(p.good_enabled);
        i.face_detected = false;
        p.update(&i);
        assert_eq!(p.value, 1.0);
        i.bad_face_page = true;
        p.update(&i);
        assert!(!p.good_enabled);
        assert!((p.value - 10.0 / 11.0).abs() < 1e-15);
    }
    #[test]
    fn show_retains_button_until_received_and_angles_are_strict() {
        let mut p = Progress {
            value: 1.0,
            good_enabled: true,
        };
        p.show();
        let mut i = input();
        i.received = false;
        p.update(&i);
        assert!(p.good_enabled);
        i.received = true;
        i.orientation = vec![std::f64::consts::FRAC_PI_6.next_up(), 0.0, 0.0];
        p.update(&i);
        assert_eq!(p.value, 0.0);
        assert!(!p.good_enabled);
        i.orientation[0] = 29.99f64.to_radians();
        p.update(&i);
        assert_eq!(p.value, 1.0 / 160.0);
    }
}
