mod points;
mod tracker;
use crate::models::RadarPoint;
pub use points::{adjust_point, sorted_points};
pub use tracker::StableTracker;

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct RawObject {
    pub t: f64,
    pub group: String,
    pub address: u32,
    pub slot: u32,
    pub quality: u16,
    pub age: u16,
    pub object_id: u16,
    pub object_class: u16,
    pub width: f64,
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub ax: f64,
}
impl RawObject {
    #[must_use]
    pub fn valid(&self) -> bool {
        self.quality >= 1
            && (0.0..=180.0).contains(&self.x)
            && (self.x > 0.2 || (1.4..=4.5).contains(&self.y.abs()))
            && self.y.abs() <= 12.0
            && !(self.vx <= -99.0 && self.x < 0.5)
    }
    #[must_use]
    pub fn point(&self, speed: f64) -> RadarPoint {
        RadarPoint {
            label: format!(
                "{}{:02}",
                if self.group == "235" { "R" } else { "R180_" },
                self.slot
            ),
            longitudinal_m: self.x,
            lateral_m: -self.y,
            source: "cornerRadar".into(),
            relative_speed_mps: Some(self.vx),
            absolute_speed_kph: Some(speed + self.vx * 3.6),
            lateral_speed_mps: Some(-self.vy),
            relative_accel_mps2: Some(self.ax),
            probability: Some(probability(self.quality)),
            valid: Some(1),
            valid_count: Some(i64::from(self.age)),
            ..RadarPoint::default()
        }
    }
}
fn probability(quality: u16) -> f64 {
    (0.35 + f64::from(quality.min(80)) / 100.0).clamp(0.35, 0.92)
}
fn unsigned(data: &[u8], start: usize, length: usize) -> u16 {
    (0..length).fold(0, |value, bit| {
        value | (u16::from((data[(start + bit) / 8] >> ((start + bit) % 8)) & 1) << bit)
    })
}
fn object(t: f64, group: &str, address: u32, slot: u32, data: &[u8], base: usize) -> RawObject {
    let width_bits = if group == "235" { 8 } else { 7 };
    let width_factor = if group == "235" { 0.01 } else { 0.05 };
    let class_bits = if group == "235" { 4 } else { 3 };
    let acceleration = unsigned(data, base + 91, 9);
    let acceleration = f64::from(acceleration) - if acceleration & 256 != 0 { 512.0 } else { 0.0 };
    RawObject {
        t,
        group: group.into(),
        address,
        slot,
        quality: unsigned(data, base, 7),
        age: unsigned(data, base + 8, 8),
        object_id: unsigned(data, base + 20, 7),
        object_class: unsigned(data, base + 36, class_bits),
        width: f64::from(unsigned(data, base + 28, width_bits)) * width_factor,
        x: f64::from(unsigned(data, base + 40, 13)) * 0.05,
        y: f64::from(unsigned(data, base + 54, 12)) * 0.05 - 102.4,
        vx: f64::from(unsigned(data, base + 67, 12)) * 0.05 - 100.0,
        vy: f64::from(unsigned(data, base + 80, 10)) * 0.05 - 25.0,
        ax: acceleration * 0.05,
    }
}
#[must_use]
pub fn decode_raw(t: f64, address: u32, data: &[u8]) -> Vec<RawObject> {
    if data.len() != 32 {
        return Vec::new();
    }
    match address {
        0x235..=0x248 => vec![object(t, "235", address, address - 0x235, data, 24)],
        0x180..=0x184 => {
            let slot = (address - 0x180) * 2;
            vec![
                object(t, "180", address, slot, data, 24),
                object(t, "180", address, slot + 1, data, 152),
            ]
        }
        _ => Vec::new(),
    }
}
