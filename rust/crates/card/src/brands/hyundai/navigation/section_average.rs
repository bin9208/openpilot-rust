//! Stock-navigation section time bank, with source odometer scale and end debounce.
use serde::{Deserialize, Serialize};
pub const DISTANCE_SCALE: f64 = 1.015;
#[derive(Default, Deserialize, Serialize)]
pub struct SectionAverage {
    pub active: bool,
    pub unlocked: bool,
    pub limit_kph: f64,
    pub start_time: f64,
    pub start_distance: f64,
    pub inactive_since: Option<f64>,
}
impl SectionAverage {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn update(&mut self, section_active: bool, limit_kph: f64, now: f64, total_distance: f64) {
        if section_active && limit_kph > 0. {
            self.inactive_since = None;
            if !self.active || limit_kph != self.limit_kph {
                self.reset();
                self.active = true;
                self.limit_kph = limit_kph;
                self.start_time = now;
                self.start_distance = total_distance;
            }
        } else if self.active {
            if let Some(since) = self.inactive_since {
                if now - since >= 3. {
                    self.reset();
                }
            } else {
                self.inactive_since = Some(now);
            }
        }
    }
    pub fn unlock(&mut self) {
        if self.active {
            self.unlocked = true;
        }
    }
    pub fn lock(&mut self) {
        self.unlocked = false;
    }
    fn limit_ms(&self) -> f64 {
        1_f64.max(self.limit_kph / 3.6)
    }
    pub fn bank_seconds(&self, now: f64, total_distance: f64) -> f64 {
        if !self.active {
            return 0.;
        }
        (now - self.start_time)
            - (total_distance - self.start_distance) * DISTANCE_SCALE / self.limit_ms()
    }
    pub fn allowance_kph(&self, now: f64, total_distance: f64) -> f64 {
        if !self.unlocked {
            return 0.;
        }
        let bank = self.bank_seconds(now, total_distance);
        if bank <= 0.5 {
            return 0.;
        }
        let remaining = 1000. / self.limit_ms() - bank;
        if remaining <= 1000. / (250. / 3.6) {
            return 250.;
        }
        250_f64.min(1000. / remaining * 3.6)
    }
}
