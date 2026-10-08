use super::constants::*;
use crate::{
    math::{maximum, median, minimum},
    point::{Point, TrackId},
};
use serde::Serialize;
use std::collections::VecDeque;

#[derive(Clone, Copy, Serialize)]
pub struct Observation {
    pub time_s: f64,
    pub d_rel: f64,
    pub y_rel: f64,
    pub v_rel: f64,
    pub v_lead: f64,
    pub yaw_rate_rad_s: f64,
    pub global_path_s: f64,
    pub d_path: f64,
}

#[derive(Clone, Serialize)]
pub struct Track {
    pub continuity_id: u64,
    pub observations: VecDeque<Observation>,
    pub front_observations: VecDeque<Observation>,
    pub front_track_id: Option<TrackId>,
    #[serde(with = "crate::wire_float::scalar")]
    pub minimum_d_rel: f64,
    #[serde(with = "crate::wire_float::scalar")]
    pub last_seen_s: f64,
    pub cutin_since_s: Option<f64>,
    #[serde(with = "crate::wire_float::scalar")]
    pub cutin_until_s: f64,
    pub risk_since_s: Option<f64>,
    #[serde(with = "crate::wire_float::scalar")]
    pub risk_until_s: f64,
    #[serde(with = "crate::wire_float::scalar")]
    pub outer_body_ambiguous_until_s: f64,
    #[serde(with = "crate::wire_float::scalar")]
    pub paired_motion_support_until_s: f64,
    #[serde(with = "crate::wire_float::scalar")]
    pub outside_until_s: f64,
    pub outside_side: f64,
}

impl Track {
    pub fn new(continuity_id: u64) -> Self {
        Self {
            continuity_id,
            observations: VecDeque::new(),
            front_observations: VecDeque::new(),
            front_track_id: None,
            minimum_d_rel: f64::INFINITY,
            last_seen_s: f64::NEG_INFINITY,
            cutin_since_s: None,
            cutin_until_s: f64::NEG_INFINITY,
            risk_since_s: None,
            risk_until_s: f64::NEG_INFINITY,
            outer_body_ambiguous_until_s: f64::NEG_INFINITY,
            paired_motion_support_until_s: f64::NEG_INFINITY,
            outside_until_s: f64::NEG_INFINITY,
            outside_side: 0.,
        }
    }
    pub fn reset(&mut self, continuity_id: u64) {
        let last_seen = self.last_seen_s;
        *self = Self::new(continuity_id);
        self.last_seen_s = last_seen;
    }
    pub fn continuous(&self, point: &Point, now: f64) -> bool {
        let Some(previous) = self.observations.back() else {
            return true;
        };
        let dt = now - previous.time_s;
        if dt <= 0. || dt > MAX_OBSERVATION_GAP_S {
            return false;
        }
        (point.d_rel - (previous.d_rel + previous.v_rel * dt)).abs() <= 2.5
            && (point.y_rel - previous.y_rel).abs() <= 1.
            && (point.v_lead - previous.v_lead).abs() <= 7.
    }
    pub fn latch(&mut self, now: f64, input: LatchInput) -> bool {
        let (since, until) = if input.risk {
            (&mut self.risk_since_s, &mut self.risk_until_s)
        } else {
            (&mut self.cutin_since_s, &mut self.cutin_until_s)
        };
        if input.raw {
            let since = since.get_or_insert(now);
            if now - *since + 1e-6 >= input.confirmation {
                *until = now + input.hold;
            }
        } else {
            *since = None;
        }
        now <= *until
    }
}

pub struct LatchInput {
    pub raw: bool,
    pub risk: bool,
    pub confirmation: f64,
    pub hold: f64,
}

pub fn values_since(observations: &VecDeque<Observation>, duration: f64) -> Vec<Observation> {
    let Some(last) = observations.back() else {
        return Vec::new();
    };
    observations
        .iter()
        .filter(|value| value.time_s >= last.time_s - duration)
        .copied()
        .collect()
}

pub fn median_slope(
    observations: &[Observation],
    field: fn(&Observation) -> f64,
    duration: f64,
) -> f64 {
    let Some(last) = observations.last() else {
        return 0.;
    };
    let values: Vec<_> = observations
        .iter()
        .filter(|value| value.time_s >= last.time_s - duration)
        .map(|value| (value.time_s, field(value)))
        .collect();
    let mut slopes = Vec::new();
    for (index, (first_time, first_value)) in values.iter().enumerate() {
        for (second_time, second_value) in &values[index + 1..] {
            let dt = second_time - first_time;
            if dt >= 0.10 {
                slopes.push((second_value - first_value) / dt);
            }
        }
    }
    median(&slopes)
}

#[derive(Clone, Copy, Default)]
pub struct Motion {
    pub rate: f64,
    pub inward_rate: f64,
    pub inward_progress: f64,
    pub travel: f64,
    pub net_fraction: f64,
    pub consistency: f64,
    pub jittering: bool,
}

pub fn motion(
    observations: &VecDeque<Observation>,
    duration: f64,
    field: fn(&Observation) -> f64,
) -> Motion {
    let values = values_since(observations, duration);
    if values.len() < 2 {
        return Motion::default();
    }
    let rate = median_slope(&values, field, duration);
    let last = field(&values[values.len() - 1]);
    let first = field(&values[0]);
    let side = 1_f64.copysign(if last != 0. {
        last
    } else if first != 0. {
        first
    } else {
        1.
    });
    let inward_rate = maximum(0., -side * rate);
    let start_count = 3.min(1.max(values.len() / 3));
    let end_count = 2.min(values.len());
    let start = median(
        &values[..start_count]
            .iter()
            .map(|value| field(value).abs())
            .collect::<Vec<_>>(),
    );
    let end = median(
        &values[values.len() - end_count..]
            .iter()
            .map(|value| field(value).abs())
            .collect::<Vec<_>>(),
    );
    let inward_progress = maximum(0., start - end);
    let mut inward_travel = 0.;
    let mut outward_travel = 0.;
    for pair in values.windows(2) {
        let delta = field(&pair[0]).abs() - field(&pair[1]).abs();
        if delta.abs() < 0.01 {
            continue;
        }
        if delta > 0. {
            inward_travel += delta;
        } else {
            outward_travel -= delta;
        }
    }
    let travel = inward_travel + outward_travel;
    let consistency = if travel > 1e-6 {
        inward_travel / travel
    } else {
        0.
    };
    let net_fraction = inward_progress / maximum(travel, 1e-6);
    Motion {
        rate,
        inward_rate,
        inward_progress,
        travel,
        net_fraction,
        consistency,
        jittering: travel >= JITTER_MIN_TRAVEL_M && net_fraction < JITTER_MIN_NET_FRACTION,
    }
}

pub fn horizon(speed: f64) -> f64 {
    maximum(1.40, minimum(3., 3. - 0.05 * maximum(0., speed)))
}

pub fn front_noise(distance: f64) -> f64 {
    let distance = maximum(0., distance);
    if distance <= 8. {
        FRONT_LATERAL_NEAR_NOISE_M
    } else if distance <= 20. {
        FRONT_LATERAL_NEAR_NOISE_M
            + (distance - 8.) / 12. * (FRONT_LATERAL_MID_NOISE_M - FRONT_LATERAL_NEAR_NOISE_M)
    } else if distance <= 45. {
        FRONT_LATERAL_MID_NOISE_M
            + (distance - 20.) / 25. * (FRONT_LATERAL_FAR_NOISE_M - FRONT_LATERAL_MID_NOISE_M)
    } else {
        FRONT_LATERAL_FAR_NOISE_M
    }
}

pub fn front_confidence(distance: f64, long: &Motion, short: &Motion) -> f64 {
    let coherence = maximum(
        0.,
        minimum(
            minimum(long.consistency, short.consistency),
            long.net_fraction,
        ),
    );
    minimum(
        1.,
        maximum(0., long.inward_progress) / front_noise(distance) * coherence,
    )
}
