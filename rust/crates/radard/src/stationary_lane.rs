//! Exact-source stopped-corner fallback (e8e152db stationary_lane.py).
use crate::{
    model::{Model, Position, VisionLead},
    point::{Identity, Point},
};
use indexmap::IndexMap;
use serde::Serialize;

fn valid_line(line: &Position) -> bool {
    line.x.len() >= 3
        && line.x.len() == line.y.len()
        && line.x.iter().chain(&line.y).all(|v| v.is_finite())
        && line.x.windows(2).all(|p| p[1] > p[0])
}
fn at(line: &Position, x: f64) -> Option<f64> {
    if !x.is_finite() || x < *line.x.first()? || x > *line.x.last()? {
        return None;
    }
    let i = line
        .x
        .partition_point(|v| *v < x)
        .clamp(1, line.x.len() - 1);
    let f = (x - line.x[i - 1]) / (line.x[i] - line.x[i - 1]);
    Some(line.y[i - 1] + f * (line.y[i] - line.y[i - 1]))
}
#[derive(Clone)]
pub struct Lane {
    left: Position,
    right: Position,
    margin: f64,
}
impl Lane {
    pub fn offset(&self, x: f64, radar_y: f64) -> Option<f64> {
        let left = at(&self.left, x)?;
        let right = at(&self.right, x)?;
        let y = -radar_y;
        if !(2.5..=4.3).contains(&(right - left))
            || !(left + self.margin..=right - self.margin).contains(&y)
        {
            return None;
        }
        let offset = (left + right) * 0.5 - y;
        (offset.abs() <= 1.).then_some(offset)
    }
}
pub fn confident_ego_lane(model: &Model) -> Option<(Lane, f64)> {
    if model.lane_change_state != "off"
        || !valid_line(&model.position)
        || model.lane_lines.len() < 3
        || model.lane_probabilities.len() < 3
        || model.lane_stds.len() < 3
    {
        return None;
    }
    if !model.lane_probabilities[1..3]
        .iter()
        .all(|v| v.is_finite() && *v >= 0.90)
        || !model.lane_stds[1..3]
            .iter()
            .all(|v| v.is_finite() && (0.0..=0.25).contains(v))
    {
        return None;
    }
    if !valid_line(&model.lane_lines[1]) || !valid_line(&model.lane_lines[2]) {
        return None;
    }
    let end = *model.position.x.last()?;
    if end < 30. {
        return None;
    }
    let lane = Lane {
        left: model.lane_lines[1].clone(),
        right: model.lane_lines[2].clone(),
        margin: 0.3 + 2. * model.lane_stds[1].max(model.lane_stds[2]),
    };
    for x in [0., 10., 20., 40_f64.min(end - 10.)] {
        lane.offset(x, -at(&model.position, x)?)?;
    }
    Some((lane, end))
}
#[derive(Clone, Serialize)]
struct History {
    point: Point,
    time_s: f64,
    since_s: f64,
    vision_since_s: Option<f64>,
    vision_samples: usize,
    last_strong_s: Option<f64>,
    confirmed: bool,
}
#[derive(Default, Serialize)]
pub struct StoppingLaneCornerTracker {
    #[serde(serialize_with = "crate::primary::entries")]
    _history: IndexMap<Identity, History>,
}
impl StoppingLaneCornerTracker {
    pub fn reset(&mut self) {
        self._history.clear();
    }
    pub fn update(
        &mut self,
        time: f64,
        ego: f64,
        model: &Model,
        points: &[Point],
        vision: Option<VisionLead>,
    ) -> Option<(Point, f64)> {
        let context = confident_ego_lane(model).filter(|_| {
            model.velocity.len() >= 2
                && model
                    .velocity
                    .last()
                    .is_some_and(|v| v.is_finite() && (-0.5..=2.).contains(v))
        });
        let (Some((lane, end)), Some(vision)) = (context, vision) else {
            self.reset();
            return None;
        };
        if !time.is_finite()
            || !ego.is_finite()
            || ego < 5.
            || ![
                vision.probability,
                vision.d_rel,
                vision.y_rel,
                vision.velocity,
            ]
            .iter()
            .all(|v| v.is_finite())
        {
            self.reset();
            return None;
        }
        let mut history = IndexMap::new();
        let mut candidates = Vec::new();
        let mut matching = 0;
        for p in points {
            if !p.corner()
                || !p.measured
                || ![p.d_rel, p.y_rel, p.v_rel, p.yv_rel, p.v_lead]
                    .iter()
                    .all(|v| v.is_finite())
                || !(30.0..=120.).contains(&p.d_rel)
                || p.v_lead.abs() > 2.
            {
                continue;
            }
            let Some(offset) = lane.offset(p.d_rel, p.y_rel) else {
                continue;
            };
            let key = p.identity();
            let old = self._history.get(&key);
            let continuous = old.is_some_and(|h| {
                let dt = time - h.time_s;
                dt > 0.
                    && dt <= 0.15
                    && (p.d_rel - h.point.d_rel - h.point.v_rel * dt).abs() <= 1.
                    && (p.y_rel - h.point.y_rel - h.point.yv_rel * dt).abs() <= 0.30
                    && (p.v_lead - h.point.v_lead).abs() <= 1.5
            });
            let mut h = History {
                point: p.clone(),
                time_s: time,
                since_s: old.filter(|_| continuous).map_or(time, |v| v.since_s),
                vision_since_s: None,
                vision_samples: 0,
                last_strong_s: None,
                confirmed: false,
            };
            let supports = vision.probability >= 0.35
                && lane.offset(vision.d_rel, vision.y_rel).is_some()
                && p.d_rel.min(vision.d_rel) >= end - 10.
                && p.d_rel.max(vision.d_rel) <= end + 25.
                && (p.d_rel - vision.d_rel).abs() <= 8_f64.min(p.d_rel * 0.12)
                && (p.y_rel - vision.y_rel).abs() <= 1.25
                && (-1.0..=8.).contains(&vision.velocity)
                && (p.v_lead - vision.velocity).abs() <= 8.;
            if supports {
                matching += 1;
                let strong = vision.probability >= 0.40;
                if let Some(old) =
                    old.filter(|h| continuous && h.last_strong_s.is_some_and(|t| time - t <= 0.15))
                {
                    h.vision_since_s = old.vision_since_s;
                    h.vision_samples = old.vision_samples;
                    h.last_strong_s = old.last_strong_s;
                    h.confirmed = old.confirmed;
                }
                if strong {
                    h.vision_since_s.get_or_insert(time);
                    h.vision_samples += 1;
                    h.last_strong_s = Some(time);
                }
                if h.vision_since_s.is_some_and(|t| time - t >= 0.10)
                    && h.last_strong_s.is_some_and(|t| time - t <= 0.15)
                    && time - h.since_s >= 0.35
                    && h.vision_samples >= 3
                    && p.d_rel <= 80.
                    && (strong || h.confirmed)
                {
                    h.confirmed = true;
                    candidates.push((p.clone(), offset));
                }
            }
            history.insert(key, h);
        }
        self._history = history;
        if candidates.len() == 1 && matching == 1 {
            candidates.pop()
        } else {
            None
        }
    }
}
