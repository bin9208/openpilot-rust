//! Tesla-only lamp observations; never writes model desires or control
//! commands. Port of carrot's `selfdrive/car/tesla_lane_signals.py`.
use crate::xiaoge::VisionResult;
use openpilot_desire::{
    indicator::{
        boundary::BoundaryIndicator,
        lane::{finite, path_direction},
        observer::Movement,
    },
    types::{Car, Model},
};

pub struct Inputs<'a> {
    pub car: &'a Car,
    pub model: &'a Model,
    pub lat_active: bool,
    pub curvature: f64,
    pub vision: Option<&'a VisionResult>,
    /// `time.monotonic_ns()` (Xiaoge's clock, not CAN boot time).
    pub now_ns: u64,
    pub allowed: bool,
    pub busy: bool,
}

pub struct LaneSignals {
    pub manual: bool,
    pub desire: bool,
    pub path: bool,
    dashed: [Option<(u64, u32)>; 2],
    stamp: u64,
    frame: Option<u32>,
    frame_time: f64,
    geometry: Option<(f64, f64)>,
    stalk_counts: Option<[u8; 2]>,
    dark_since: Option<f64>,
    prediction_consumed: bool,
    boundary: BoundaryIndicator,
    active: Option<Movement>,
    request_id: u64,
    candidate: Option<(Option<(u8, &'static str)>, f64, u32)>,
    pub reason: &'static str,
}

impl LaneSignals {
    pub fn new(manual: bool, desire: bool, path: bool) -> Self {
        Self {
            manual,
            desire,
            path,
            dashed: [None, None],
            stamp: 0,
            frame: None,
            frame_time: 0.,
            geometry: None,
            stalk_counts: None,
            dark_since: None,
            prediction_consumed: false,
            boundary: BoundaryIndicator::default(),
            active: None,
            request_id: 1 << 31,
            candidate: None,
            reason: "idle",
        }
    }

    fn vision(&mut self, vision: Option<&VisionResult>, now: u64) {
        let received = vision.and_then(|v| v.lane_received).unwrap_or(0);
        let valid = vision.is_some_and(|v| v.lane_valid)
            && 0 < received
            && received <= now
            && now - received <= 800_000_000;
        let stamp = if valid { received } else { 0 };
        if !valid || stamp < self.stamp || stamp - self.stamp > 800_000_000 {
            self.dashed = [None, None];
        }
        let lanes = match vision.filter(|_| valid) {
            Some(v) => [v.left_lane, v.right_lane],
            None => [-1, -1],
        };
        for (i, value) in lanes.into_iter().enumerate() {
            if value != 0 {
                self.dashed[i] = None;
            } else if stamp > self.stamp {
                let (first, count) = self.dashed[i].unwrap_or((stamp, 0));
                self.dashed[i] = Some((first, count + 1));
            }
        }
        self.stamp = stamp;
    }

    /// Uncoloured dashed classification plus visible adjacent lane/road support.
    fn adjacent(
        dashed: &[Option<(u64, u32)>; 2],
        stamp: u64,
        car: &Car,
        md: &Model,
        direction: u8,
    ) -> bool {
        if !matches!(direction, 1 | 2) {
            return false;
        }
        let Some((first, count)) = dashed[usize::from(direction - 1)] else {
            return false;
        };
        if count < 2 || stamp.saturating_sub(first) < 200_000_000 {
            return false;
        }
        let (inner, outer, edge) = if direction == 1 { (1, 0, 0) } else { (2, 3, 1) };
        let sign = if direction == 1 { -1. } else { 1. };
        if ![inner, outer]
            .iter()
            .all(|i| md.lane_line_probs[*i].is_finite() && md.lane_line_probs[*i] >= 0.5)
        {
            return false;
        }
        let std = md.road_edge_stds[edge];
        if !std.is_finite() || !(0. ..=1.).contains(&std) {
            return false;
        }
        for index in [0, 4, 8] {
            let (Some(&boundary), Some(&outside), Some(&road)) = (
                md.lane_lines[inner].get(index),
                md.lane_lines[outer].get(index),
                md.road_edges[edge].get(index),
            ) else {
                return false;
            };
            if !finite(&[boundary, outside, road]) {
                return false;
            }
            if !(2.5..=4.8).contains(&(sign * (outside - boundary)))
                || sign * (road - outside) < 0.3
            {
                return false;
            }
        }
        !(if direction == 1 {
            car.left_blindspot
        } else {
            car.right_blindspot
        })
    }

    fn stop(&mut self, reason: &'static str) -> Option<(u8, u64)> {
        if self.active.is_some() {
            self.boundary.block_until = self.boundary.block_until.max(self.frame_time + 3.);
        }
        self.active = None;
        self.candidate = None;
        self.boundary.reset_geometry();
        self.boundary.request_direction = 0;
        self.reason = reason;
        None
    }

    pub fn update(&mut self, i: Inputs<'_>) -> Option<(u8, u64)> {
        let now = i.now_ns as f64 / 1e9;
        let (cs, md) = (i.car, i.model);
        self.vision(i.vision, i.now_ns);
        if cs.left_blinker || cs.right_blinker || !i.allowed {
            self.dark_since = None;
        } else if self.dark_since.is_none() {
            self.dark_since = Some(now);
        }
        let counts = cs.stalk_counts;
        let pressed = self.stalk_counts.is_some_and(|c| c != counts);
        self.stalk_counts = Some(counts);
        if pressed {
            self.boundary.block_until = now + 3.;
        }
        let common = i.allowed
            && !i.busy
            && cs.can_valid
            && !cs.vehicle_sensors_invalid
            && cs.drive
            && !cs.trailer_connected
            && !cs.steer_fault_temporary
            && !cs.steer_fault_permanent
            && cs.turn_signal_driver_valid
            && cs.turn_signal_driver_direction == 0
            && !pressed
            && cs.blinker_lever == 0
            && !(cs.left_blinker && cs.right_blinker)
            && cs.v_ego.is_finite()
            && cs.v_ego >= 30. / 3.6
            && cs.steering_torque.is_finite();
        if !common || !(self.manual || self.desire || self.path) {
            return self.stop("context");
        }
        if self.active.as_ref().is_some_and(|a| a.source != "manual") && !i.lat_active {
            return self.stop("lateral_inactive");
        }
        let (dashed, stamp) = (self.dashed, self.stamp);
        let adjacent = |direction: u8| Self::adjacent(&dashed, stamp, cs, md, direction);
        if self.frame == Some(md.frame_id) {
            if !(0. ..=0.15).contains(&(now - self.frame_time)) {
                return self.stop("model_stale");
            }
            if let Some(a) = &self.active {
                if adjacent(a.direction) {
                    return Some((a.direction, self.request_id));
                }
                return self.stop("lane_evidence");
            }
            return None;
        }
        if self.frame.is_some_and(|f| md.frame_id < f)
            || (self.frame.is_some() && !(0.02..=0.15).contains(&(now - self.frame_time)))
        {
            self.frame = Some(md.frame_id);
            self.frame_time = now;
            return self.stop("model_discontinuity");
        }
        self.frame = Some(md.frame_id);
        self.frame_time = now;
        if let Some(mut active) = self.active.take() {
            let opposite = if active.direction == 1 {
                cs.right_blinker
            } else {
                cs.left_blinker
            };
            let enabled = if active.source == "manual" {
                self.manual
            } else {
                i.lat_active
                    && if active.source == "model_desire" {
                        self.desire
                    } else {
                        self.path
                    }
            };
            if !enabled || opposite || !adjacent(active.direction) || !active.update(now, cs, md) {
                return self.stop("completed_or_cancelled");
            }
            self.reason = active.source;
            let direction = active.direction;
            self.active = Some(active);
            return Some((direction, self.request_id));
        }
        let geometry = (
            md.lane_lines[1].first().copied(),
            md.lane_lines[2].first().copied(),
        );
        let (Some(left), Some(right)) = geometry else {
            return self.stop("geometry");
        };
        let old = self.geometry.replace((left, right));
        if !finite(&[left, right])
            || !(left < 0. && 0. < right)
            || !(2.5 < right - left && right - left < 4.8)
            || old.is_some_and(|(l, r)| (l - left).abs().max((r - right).abs()) > 0.4)
        {
            return self.stop("geometry");
        }
        let manual = self.manual;
        self.boundary
            .update_with(now, cs, md, manual, true, false, &|c, m, d, _| {
                Self::adjacent(&dashed, stamp, c, m, d)
            });
        let mut direction = self.boundary.request_direction;
        let mut source = "manual";
        if direction == 0
            && i.lat_active
            && !cs.steering_pressed
            && !(cs.left_blinker || cs.right_blinker)
            && now >= self.boundary.block_until
        {
            let mut candidate = None;
            let probs = &md.desire_state;
            if self.desire
                && probs
                    .iter()
                    .all(|p| p.is_finite() && (0. ..=1.).contains(p))
            {
                let k = if probs[3] > probs[4] { 3 } else { 4 };
                let best = probs[..3].iter().copied().fold(f64::NEG_INFINITY, f64::max);
                if probs[k] >= 0.5 && probs[k] >= best && probs[k] - probs[7 - k] >= 0.3 {
                    candidate = Some((if k == 3 { 1 } else { 2 }, "model_desire"));
                }
            }
            if candidate.is_none() && self.path {
                let predicted = path_direction(md, i.curvature);
                if predicted != 0 {
                    candidate = Some((predicted, "model_path"));
                }
            }
            if candidate.is_none() && self.dark_since.is_some_and(|s| now - s >= 1.) {
                self.prediction_consumed = false;
            }
            if candidate.is_some_and(|(d, _)| !adjacent(d)) {
                candidate = None;
            }
            self.candidate = match self.candidate {
                Some((old, since, count)) if old == candidate => {
                    Some((candidate, since, count + 1))
                }
                _ => Some((candidate, now, 1)),
            };
            if let (Some((d, s)), Some((_, since, count))) = (candidate, self.candidate) {
                if !self.prediction_consumed && count >= 4 && now - since >= 0.2 {
                    direction = d;
                    source = s;
                }
            }
        } else {
            self.candidate = None;
        }
        if direction != 0 {
            self.active = Movement::new(now, direction, source, md);
            self.request_id += 1;
            self.prediction_consumed = true;
            self.boundary.own(now, direction);
            self.reason = source;
            self.candidate = None;
            return Some((direction, self.request_id));
        }
        self.reason = "observing";
        None
    }
}
