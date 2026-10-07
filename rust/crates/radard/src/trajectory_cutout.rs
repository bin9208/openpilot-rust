use crate::{
    math::{float_sum, maximum, minimum, square},
    model::VisionLead,
    path::Path,
    point::Point,
    Error,
};
use serde::Serialize;
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct Prediction {
    pub time_s: f64,
    pub confidence: f64,
}

#[derive(Clone, Serialize)]
struct Observation {
    time_s: f64,
    point: Point,
    lateral: Point,
    d_path: f64,
}

#[derive(Default, Serialize)]
pub struct Tracker {
    _history: VecDeque<Observation>,
    _vision_since: Option<f64>,
    _vision_anchor: Option<f64>,
    _candidate_since: Option<f64>,
}

pub struct Input<'a> {
    pub time_s: f64,
    pub point: Option<&'a Point>,
    pub lateral: Option<&'a Point>,
    pub vision: Option<&'a VisionLead>,
    pub path: Option<&'a Path>,
    pub v_ego: f64,
    pub yaw_rate: f64,
}

impl Tracker {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn inactive(&mut self) -> Prediction {
        self._candidate_since = None;
        Prediction::default()
    }

    pub fn update(&mut self, input: Input<'_>) -> Result<Prediction, Error> {
        let Input {
            time_s: now,
            point,
            lateral,
            vision,
            path,
            v_ego,
            yaw_rate,
        } = input;
        let (Some(point), Some(lateral), Some(path)) = (point, lateral, path) else {
            self.reset();
            return Ok(Prediction::default());
        };
        if !point.measured
            || !lateral.measured
            || point.source != "frontRadar"
            || point.track_id.0 == 0
            || point.radar_track_state < 2
            || ![
                now,
                v_ego,
                yaw_rate,
                point.d_rel,
                point.y_rel,
                point.v_rel,
                point.v_lead,
                point.a_lead,
                lateral.d_rel,
                lateral.y_rel,
                lateral.v_rel,
            ]
            .iter()
            .all(|value| value.is_finite())
            || path.point_count() < 2
            || yaw_rate.abs() >= 0.025
            || !(5. ..=40.).contains(&v_ego)
            || !(6. < point.d_rel && point.d_rel <= 60.)
            || point.v_lead <= 4.
            || point.a_lead < -2.5
        {
            self.reset();
            return Ok(Prediction::default());
        }
        let d_path = path.project(lateral.d_rel, lateral.y_rel).d_path;
        let front_path = path.project(point.d_rel, point.y_rel).d_path;
        if !d_path.is_finite()
            || (d_path - front_path).abs() > 0.65
            || (d_path - lateral.y_rel).abs() > 0.75
        {
            self.reset();
            return Ok(Prediction::default());
        }
        if let Some(previous) = self._history.back() {
            let dt = now - previous.time_s;
            if !(0. < dt && dt <= 0.15)
                || point.track_id != previous.point.track_id
                || lateral.identity() != previous.lateral.identity()
                || (point.d_rel - previous.point.d_rel - previous.point.v_rel * dt).abs() > 1.5
                || (point.v_lead - previous.point.v_lead).abs() > 3.
                || (lateral.y_rel - previous.lateral.y_rel).abs() > 0.65
            {
                self.reset();
            }
        }
        self._history.push_back(Observation {
            time_s: now,
            point: point.clone(),
            lateral: lateral.clone(),
            d_path,
        });
        while self
            ._history
            .front()
            .is_some_and(|observation| now - observation.time_s > 0.40 + 0.15)
        {
            self._history.pop_front();
        }
        let valid_vision = vision.filter(|vision| {
            [vision.probability, vision.d_rel, vision.y_rel, vision.x_std]
                .iter()
                .all(|value| value.is_finite())
                && vision.probability >= 0.40
                && vision.x_std >= 0.
        });
        let same = valid_vision.is_some_and(|vision| {
            (vision.d_rel - point.d_rel).abs() <= 3.
                && (vision.y_rel - point.y_rel).abs() <= 1.
                && front_path.abs() < 0.5
        });
        if same {
            let since = self._vision_since.get_or_insert(now);
            if now - *since >= 0.30 {
                self._vision_anchor = Some(now);
            }
        } else {
            self._vision_since = None;
        }
        if self._vision_anchor.is_none_or(|anchor| now - anchor > 2.)
            || valid_vision.is_none_or(|vision| {
                vision.d_rel - point.d_rel <= maximum(maximum(3., 0.25 * point.d_rel), vision.x_std)
            })
        {
            return Ok(self.inactive());
        }
        let history: Vec<_> = self
            ._history
            .iter()
            .filter(|observation| now - observation.time_s <= 0.40)
            .collect();
        let recent: Vec<_> = history
            .iter()
            .copied()
            .filter(|observation| now - observation.time_s <= 0.20)
            .collect();
        if history.len() < 5 || recent.len() < 3 || now - history[0].time_s < 0.25 {
            return Ok(self.inactive());
        }
        let side = 1_f64.copysign(d_path);
        let net = side * (d_path - history[0].d_path);
        let travel = float_sum(
            history
                .windows(2)
                .map(|pair| (pair[1].d_path - pair[0].d_path).abs()),
        );
        let long_rate = net / (now - history[0].time_s);
        let short_rate = side * (d_path - recent[0].d_path) / (now - recent[0].time_s);
        let raw_progress = side * (lateral.y_rel - history[0].lateral.y_rel);
        let front_progress = side * (point.y_rel - history[0].point.y_rel);
        let latest = side * (d_path - history[history.len() - 2].d_path);
        let front_min = if lateral.corner() { -0.10 } else { 0.10 };
        if !(0.35 <= d_path.abs() && d_path.abs() < 2.15)
            || net < 0.20
            || raw_progress < 0.15
            || front_progress < front_min
            || net < 0.75 * travel
            || latest < -0.03
            || minimum(long_rate, short_rate) < 0.35
        {
            return Ok(self.inactive());
        }
        let exit_time = (2.15 - d_path.abs()) / minimum(long_rate, short_rate);
        let horizon = exit_time + 0.30;
        let gap = point.d_rel
            + minimum(0., point.v_rel) * horizon
            + 0.5 * (minimum(-0.5, point.a_lead) - 0.5) * square(horizon)?;
        if !(0. < exit_time && exit_time <= 2.5) || gap <= 6. {
            return Ok(self.inactive());
        }
        let since = self._candidate_since.get_or_insert(now);
        let confidence = minimum(1., maximum(0., (now - *since - 0.10) / 0.30));
        Ok(Prediction {
            time_s: exit_time,
            confidence,
        })
    }
}
