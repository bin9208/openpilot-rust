//! Advisory-only paired front/corner stopped obstacle warning from pinned source.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Point {
    pub source: String,
    pub track_id: u64,
    pub d: f64,
    pub y: f64,
    pub v: f64,
    pub yv: f64,
    pub speed: f64,
    pub measured: bool,
    pub track_state: i32,
}
use openpilot_radard::stationary_lane::Lane;
#[derive(Serialize)]
pub struct Warning {
    previous: Option<[Point; 2]>,
    since: Option<f64>,
    last_sample: Option<f64>,
    last_pair_time: Option<f64>,
    last_alert: f64,
    armed: bool,
    pub last_radar_time: Option<f64>,
}
impl Default for Warning {
    fn default() -> Self {
        Self {
            previous: None,
            since: None,
            last_sample: None,
            last_pair_time: None,
            last_alert: f64::NEG_INFINITY,
            armed: true,
            last_radar_time: None,
        }
    }
}
impl Warning {
    pub fn clear(&mut self) {
        self.previous = None;
        self.since = None;
        self.last_sample = None;
    }
    #[allow(clippy::too_many_arguments)] // Direct source advisory contract.
    pub fn update(
        &mut self,
        now: f64,
        v_ego: f64,
        lane: Option<&Lane>,
        vision_probs: &[f64],
        points: &[Point],
        enabled: bool,
        selected_lead: bool,
    ) -> bool {
        if !now.is_finite() || self.last_sample.is_some_and(|t| now <= t) {
            self.clear();
            return false;
        }
        let mut qualified = Vec::new();
        if let Some(lane) = lane.filter(|_| enabled && v_ego.is_finite() && v_ego >= 10.) {
            for p in points {
                if p.measured
                    && [p.d, p.y, p.v, p.yv, p.speed].iter().all(|v| v.is_finite())
                    && (15. ..=150.).contains(&p.d)
                    && p.speed.abs() <= 2.
                    && p.v <= -8.
                    && lane.offset(p.d, p.y).is_some()
                {
                    qualified.push(p);
                }
            }
        }
        let fronts: Vec<_> = qualified
            .iter()
            .filter(|p| p.source == "frontRadar" && p.track_state >= 2)
            .collect();
        let corners: Vec<_> = qualified
            .iter()
            .filter(|p| p.source.starts_with("corner"))
            .collect();
        let mut pairs = Vec::new();
        for f in fronts {
            for c in &corners {
                if (f.d - c.d).abs() <= 3.
                    && (f.y - c.y).abs() <= 0.75
                    && (f.speed - c.speed).abs() <= 2.
                {
                    pairs.push([(*f).clone(), (**c).clone()]);
                }
            }
        }
        if !pairs.is_empty() {
            self.last_pair_time = Some(now);
        } else if self.last_pair_time.is_some_and(|t| now - t >= 5.) && now - self.last_alert >= 30.
        {
            self.armed = true;
        }
        let vision_absent = !vision_probs.is_empty()
            && vision_probs
                .iter()
                .all(|p| p.is_finite() && (0. ..0.4).contains(p));
        if !enabled || !vision_absent || selected_lead || pairs.len() != 1 {
            self.clear();
            return false;
        }
        let pair = pairs.pop().expect("unique pair");
        let dt = self.last_sample.map_or(f64::INFINITY, |t| now - t);
        let continuous = self.previous.as_ref().is_some_and(|old| {
            dt > 0.
                && dt <= 0.15
                && pair.iter().zip(old).all(|(p, o)| {
                    p.source == o.source
                        && p.track_id == o.track_id
                        && (p.d - o.d - o.v * dt).abs() <= 1.
                        && (p.y - o.y - o.yv * dt).abs() <= 0.3
                        && (p.speed - o.speed).abs() <= 1.5
                })
        });
        if !continuous {
            self.since = Some(now);
        }
        let ttc = pair
            .iter()
            .map(|p| p.d / -p.v)
            .fold(f64::NEG_INFINITY, f64::max);
        let max_d = pair.iter().map(|p| p.d).fold(f64::NEG_INFINITY, f64::max);
        self.previous = Some(pair);
        self.last_sample = Some(now);
        if self.armed
            && now - self.since.expect("confirmation initialized") >= 0.5
            && ttc <= 4.8
            && max_d <= 120.
            && now - self.last_alert >= 30.
        {
            self.last_alert = now;
            self.armed = false;
            true
        } else {
            false
        }
    }
}
