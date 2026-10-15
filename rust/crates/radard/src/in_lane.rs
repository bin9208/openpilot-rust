//! Measured braking corner corroboration from e8e152db in_lane.py.
use crate::{
    model::{Model, VisionLead},
    point::{Identity, Point},
    stationary_lane::confident_ego_lane,
};
use indexmap::IndexMap;
use serde::Serialize;
#[derive(Clone, Serialize)]
struct History {
    point: Point,
    time_s: f64,
    since_s: f64,
    vision_since_s: Option<f64>,
    vision_samples: usize,
    lane_since_s: Option<f64>,
    braking_since_s: Option<f64>,
    braking_start_v: f64,
    observations: Vec<[f64; 3]>,
}
#[derive(Default, Serialize)]
pub struct InLaneCornerTracker {
    #[serde(serialize_with = "crate::primary::entries")]
    _history: IndexMap<Identity, History>,
    _measurement_time_s: Option<f64>,
}
impl InLaneCornerTracker {
    pub fn reset(&mut self) {
        self._history.clear();
        self._measurement_time_s = None;
    }
    pub fn update(
        &mut self,
        time: f64,
        ego: f64,
        model: &Model,
        points: &[Point],
        vision: Option<VisionLead>,
        measurement: Option<f64>,
    ) -> Option<(Point, f64)> {
        if !time.is_finite() || !ego.is_finite() || ego < 5. {
            self.reset();
            return None;
        }
        let measurement = measurement.unwrap_or(time);
        if self._measurement_time_s == Some(measurement) {
            return None;
        }
        if !measurement.is_finite() || self._measurement_time_s.is_some_and(|t| measurement < t) {
            self.reset();
            self._measurement_time_s = Some(measurement);
            return None;
        }
        self._measurement_time_s = Some(measurement);
        let mut history = IndexMap::new();
        for p in points {
            if !p.corner()
                || !p.measured
                || ![p.d_rel, p.y_rel, p.v_rel, p.yv_rel, p.v_lead, p.a_lead]
                    .iter()
                    .all(|v| v.is_finite())
                || !(5.0..=120.).contains(&p.d_rel)
            {
                continue;
            }
            let key = p.identity();
            let old = self._history.get(&key);
            let old = old.filter(|h| {
                let dt = time - h.time_s;
                dt > 0.
                    && dt <= 0.15
                    && (p.d_rel - h.point.d_rel - h.point.v_rel * dt).abs() <= 1.
                    && (p.y_rel - h.point.y_rel - h.point.yv_rel * dt).abs() <= 0.40
                    && (p.v_lead - h.point.v_lead).abs() <= 1.5
            });
            let mut item = if let Some(h) = old {
                History {
                    point: p.clone(),
                    time_s: time,
                    observations: h
                        .observations
                        .iter()
                        .filter(|v| time - v[0] <= 0.5)
                        .copied()
                        .chain(std::iter::once([time, p.d_rel, p.v_rel]))
                        .collect(),
                    ..h.clone()
                }
            } else {
                History {
                    point: p.clone(),
                    time_s: time,
                    since_s: time,
                    vision_since_s: None,
                    vision_samples: 0,
                    lane_since_s: None,
                    braking_since_s: None,
                    braking_start_v: p.v_lead,
                    observations: vec![[time, p.d_rel, p.v_rel]],
                }
            };
            if p.a_lead > -0.5 || p.v_rel >= -1. {
                item.braking_since_s = None;
            } else if item.braking_since_s.is_none() {
                item.braking_since_s = Some(time);
                item.braking_start_v = p.v_lead;
            }
            history.insert(key, item);
        }
        self._history = history;
        let context = confident_ego_lane(model);
        let slowing = model.velocity.len() >= 2
            && model
                .velocity
                .last()
                .is_some_and(|v| v.is_finite() && *v >= -0.5 && *v <= 0.75 * ego);
        let mut offsets = IndexMap::new();
        for (key, item) in &mut self._history {
            let offset = context
                .as_ref()
                .and_then(|(lane, _)| lane.offset(item.point.d_rel, item.point.y_rel));
            offsets.insert(key.clone(), offset);
            if offset.is_none() {
                item.lane_since_s = None;
            } else {
                item.lane_since_s.get_or_insert(time);
            }
        }
        let vision = vision.filter(|v| {
            slowing
                && context.is_some()
                && [
                    v.probability,
                    v.d_rel,
                    v.y_rel,
                    v.velocity,
                    v.x_std,
                    v.y_std,
                    v.v_std,
                ]
                .iter()
                .all(|v| v.is_finite())
                && v.probability >= 0.50
                && v.x_std > 0.
                && v.x_std <= 15.
                && v.y_std > 0.
                && v.y_std <= 2.
                && v.v_std > 0.
                && v.v_std <= 5.
        });
        let Some(vision) = vision else {
            self.clear_vision();
            return None;
        };
        let (lane, _) = context.as_ref()?;
        let visual_offset = lane.offset(vision.d_rel, vision.y_rel);
        let mut matches = Vec::new();
        for (key, item) in &mut self._history {
            if let (Some(offset), Some(visual)) = (offsets[key], visual_offset) {
                if (offset - visual).abs() <= 0.75
                    && (item.point.d_rel - vision.d_rel).abs() <= 10.
                    && (item.point.v_lead - vision.velocity).abs() <= 4.
                {
                    matches.push((key.clone(), offset));
                    continue;
                }
            }
            item.vision_since_s = None;
            item.vision_samples = 0;
        }
        if matches.len() != 1 {
            self.clear_vision();
            return None;
        }
        let (key, offset) = matches.pop()?;
        let item = self._history.get_mut(&key)?;
        let since = *item.vision_since_s.get_or_insert(time);
        item.vision_samples += 1;
        let p = &item.point;
        let samples = &item.observations;
        let displacement: f64 = samples
            .windows(2)
            .map(|v| 0.5 * (v[0][2] + v[1][2]) * (v[1][0] - v[0][0]))
            .sum();
        let duration = time - samples[0][0];
        let consistent=duration>=0.30 && (p.d_rel-samples[0][1]-displacement).abs()<=crate::primary::constants::RADAR_ONLY_MOVING_CORNER_MAX_LONGITUDINAL_ERROR_RATE_MPS*duration;
        if !consistent
            || item.braking_start_v - p.v_lead < 0.30
            || !item.braking_since_s.is_some_and(|t| time - t >= 0.35)
            || !item.lane_since_s.is_some_and(|t| time - t >= 0.35)
            || time - item.since_s < 0.35
            || item.vision_samples < 3
            || time - since < 0.10 - 1e-6
            || !(10.0..=80.).contains(&p.d_rel)
            || p.v_rel >= -1.
            || p.v_lead < -2.
        {
            return None;
        }
        Some((p.clone(), offset))
    }
    fn clear_vision(&mut self) {
        for item in self._history.values_mut() {
            item.vision_since_s = None;
            item.vision_samples = 0;
        }
    }
}
