//! Stock navigation camera policy, ported from pinned Hyundai navi_speedcam.py.
use serde::{Deserialize, Serialize};
#[derive(Clone, Deserialize, Serialize)]
pub struct Preview {
    pub kind: u32,
    pub speed: u32,
    pub flagged: bool,
    pub received: f64,
    pub target: f64,
    pub saturated: bool,
}
#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
pub struct SpeedcamPolicy {
    pub previews: Vec<Preview>,
    pub early_lead: bool,
    pub warning_active: bool,
    pub warning_speed: f64,
    pub warning_start_distance: Option<f64>,
    pub warning_class: Option<u32>,
    pub skipped: bool,
}
pub fn decode_camera_profile(value: u32, offset: f64) -> Option<(u32, u32, bool)> {
    if value == 0 || value == u32::MAX || !(0. < offset && offset < 8191.) {
        return None;
    }
    let low = value & 0x1ff;
    let kind = low & 0xf;
    let code = low >> 4;
    if kind > 3 || !(2..=31).contains(&code) {
        return None;
    }
    Some((kind, (code - 1) * 5, value >> 9 != 0))
}
impl SpeedcamPolicy {
    pub fn reset_warning(&mut self) {
        self.warning_active = false;
        self.warning_speed = 0.;
        self.warning_start_distance = None;
        self.warning_class = None;
        self.skipped = false;
    }
    pub fn clear(&mut self) {
        self.previews.clear();
        self.reset_warning();
    }
    pub fn add_preview(&mut self, value: u32, offset: f64, total_distance: f64) {
        let Some((kind, speed, flagged)) = decode_camera_profile(value, offset) else {
            return;
        };
        let target = total_distance + offset;
        let saturated = offset >= 1985.;
        self.previews
            .retain(|p| total_distance - p.received <= 3000.);
        for p in &mut self.previews {
            if (p.kind, p.speed, p.flagged) == (kind, speed, flagged)
                && (p.target - target).abs() < 20.
            {
                p.received = total_distance;
                if !saturated || p.saturated {
                    p.target = target;
                }
                p.saturated = p.saturated && saturated;
                return;
            }
        }
        self.previews.push(Preview {
            kind,
            speed,
            flagged,
            received: total_distance,
            target,
            saturated,
        });
    }
    pub fn classify(
        &self,
        speed: f64,
        total_distance: f64,
        starting: bool,
        start_distance: Option<f64>,
    ) -> Option<u32> {
        let lead = speed * 6.1;
        let tolerance = 60_f64.max(lead * 0.2);
        let start_distance = start_distance.unwrap_or(total_distance);
        let mut far = lead + tolerance;
        let mut zone_far = lead + 60.;
        if self.early_lead {
            far = far.max(lead * 1.35);
            zone_far = zone_far.max(lead * 1.35);
        }
        let mut candidates = Vec::new();
        let mut late_hard = Vec::new();
        for p in &self.previews {
            if f64::from(p.speed) != speed || total_distance - p.received > 2600. {
                continue;
            }
            let start_ahead = p.target - start_distance;
            if p.kind == 3 && !p.flagged && !(lead - 60. <= start_ahead && start_ahead <= zone_far)
            {
                continue;
            }
            let ahead = p.target - total_distance;
            let matched = if p.saturated {
                let matched = starting && lead - tolerance <= ahead && ahead <= far;
                if !starting
                    && !matched
                    && (p.flagged || p.kind <= 1)
                    && -50. <= ahead
                    && ahead <= far
                {
                    late_hard.push(p);
                }
                matched
            } else {
                -50. <= ahead && ahead <= far
            };
            if matched {
                candidates.push(p);
            }
        }
        if !late_hard.is_empty()
            && !self.previews.iter().any(|p| {
                f64::from(p.speed) == speed
                    && !p.flagged
                    && matches!(p.kind, 2 | 3)
                    && total_distance < p.target
                    && p.target <= start_distance + far
            })
        {
            candidates.extend(late_hard);
        }
        if candidates.is_empty() {
            None
        } else if candidates.iter().any(|p| p.flagged || p.kind <= 1) {
            Some(3)
        } else if candidates.iter().any(|p| p.kind == 2) {
            Some(2)
        } else {
            Some(1)
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        warning_active: bool,
        warning_speed: f64,
        total_distance: f64,
        accel_rising: bool,
        mobile_zone_decel: bool,
        skip_box: bool,
        skip_mobile_zone: bool,
        skip_unknown: bool,
    ) -> (bool, bool) {
        if !warning_active || warning_speed <= 0. {
            self.reset_warning();
            return (false, false);
        }
        let starting = !self.warning_active || warning_speed != self.warning_speed;
        if starting {
            self.reset_warning();
            self.warning_active = true;
            self.warning_speed = warning_speed;
            self.warning_start_distance = Some(total_distance);
        }
        if let Some(current) = self.classify(
            warning_speed,
            total_distance,
            starting,
            self.warning_start_distance,
        ) {
            if self.warning_class.is_none_or(|old| current > old) {
                self.warning_class = Some(current);
            }
        }
        let class = self.warning_class.unwrap_or(3);
        if class == 1 && !mobile_zone_decel {
            return (true, false);
        }
        let skippable = (class == 2 && skip_box)
            || (class == 1 && skip_mobile_zone)
            || (self.warning_class.is_none() && skip_unknown && warning_speed > 30.);
        let consume = skippable && accel_rising && !self.skipped;
        if consume {
            self.skipped = true;
        }
        (self.skipped && skippable, consume)
    }
}
