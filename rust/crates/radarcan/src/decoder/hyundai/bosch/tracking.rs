//! Unsmooth raw-return identity association with atomic validation and coasting.
use super::{graph, raw::RawDetection};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub distance_gate_m: f64,
    pub speed_gate_mps: f64,
    pub bearing_near_deg: f64,
    pub bearing_mid_deg: f64,
    pub bearing_far_deg: f64,
    pub bearing_distant_deg: f64,
    pub coast_s: f64,
    pub same_slot_bonus: f64,
    pub unmatched_cost: f64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            distance_gate_m: 3.5,
            speed_gate_mps: 2.,
            bearing_near_deg: 12.,
            bearing_mid_deg: 5.,
            bearing_far_deg: 2.5,
            bearing_distant_deg: 2.,
            coast_s: 0.3,
            same_slot_bonus: 0.03,
            unmatched_cost: 0.65,
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("distance_gate_m", self.distance_gate_m),
            ("speed_gate_mps", self.speed_gate_mps),
            ("bearing_near_deg", self.bearing_near_deg),
            ("bearing_mid_deg", self.bearing_mid_deg),
            ("bearing_far_deg", self.bearing_far_deg),
            ("bearing_distant_deg", self.bearing_distant_deg),
            ("coast_s", self.coast_s),
            ("same_slot_bonus", self.same_slot_bonus),
            ("unmatched_cost", self.unmatched_cost),
        ] {
            if !value.is_finite() {
                return Err(format!("{name} must be finite"));
            }
            if name == "same_slot_bonus" {
                if !(0. ..=0.1).contains(&value) {
                    return Err("same_slot_bonus must remain a weak preference in [0, 0.1]".into());
                }
            } else if value <= 0. {
                return Err(format!("{name} must be positive"));
            }
        }
        for (name, value) in [
            ("bearing_near_deg", self.bearing_near_deg),
            ("bearing_mid_deg", self.bearing_mid_deg),
            ("bearing_far_deg", self.bearing_far_deg),
            ("bearing_distant_deg", self.bearing_distant_deg),
        ] {
            if value > 180. {
                return Err(format!("{name} must be <= 180 degrees"));
            }
        }
        Ok(())
    }
    fn bearing_gate(&self, distance: f64) -> f64 {
        let degrees = if distance < 15. {
            self.bearing_near_deg
        } else if distance < 30. {
            self.bearing_mid_deg
        } else if distance < 60. {
            self.bearing_far_deg
        } else {
            self.bearing_distant_deg
        };
        degrees * (std::f64::consts::PI / 180.)
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Track {
    pub raw_track_id: u32,
    pub detection: RawDetection,
    pub age_scans: usize,
    pub recovered: bool,
    pub previous_slot: Option<usize>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct State {
    pub track: Track,
    pub x: f64,
    pub y: f64,
    pub last_seen_scan_ns: i64,
    pub last_seen_update: usize,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Decision {
    pub timestamp_ns: i64,
    pub raw_track_id: u32,
    pub slot: usize,
    pub previous_slot: Option<usize>,
    pub created: bool,
    pub recovered: bool,
    pub age_scans: usize,
    pub d_rel: f64,
    pub y_rel: f64,
    pub v_rel: f64,
    pub predicted_d_rel: Option<f64>,
    pub predicted_y_rel: Option<f64>,
    pub residual_d_m: Option<f64>,
    pub residual_bearing_rad: Option<f64>,
    pub bearing_gate_rad: Option<f64>,
    pub chosen_cost: Option<f64>,
    pub best_alternative_cost: Option<f64>,
    pub candidate_count: usize,
    pub identity_ambiguous: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Manager {
    pub config: Config,
    pub next_id: u32,
    pub last_timestamp_ns: Option<i64>,
    #[serde(rename = "_update_count")]
    pub update_count: usize,
    #[serde(rename = "_states")]
    pub states: BTreeMap<u32, State>,
    pub last_pair_possible: usize,
    pub last_pair_candidates: usize,
    pub last_component_count: usize,
    pub last_largest_component: usize,
    pub last_fast_components: usize,
    pub last_tied_components: usize,
    pub last_ambiguous_detections: usize,
    pub last_solver_fallback: bool,
    pub stats: BTreeMap<String, usize>,
    pub trace_decisions: bool,
    pub last_decisions: Vec<Decision>,
    pub last_identity_evidence: BTreeMap<u32, (i64, usize, bool)>,
}
impl Manager {
    pub fn new(config: Config, first_id: u32) -> Result<Self, String> {
        config.validate()?;
        if !(1..=i32::MAX as u32).contains(&first_id) {
            return Err(format!("first_id must be an integer in [1, {}]", i32::MAX));
        }
        Ok(Self {
            config,
            next_id: first_id,
            last_timestamp_ns: None,
            update_count: 0,
            states: BTreeMap::new(),
            last_pair_possible: 0,
            last_pair_candidates: 0,
            last_component_count: 0,
            last_largest_component: 0,
            last_fast_components: 0,
            last_tied_components: 0,
            last_ambiguous_detections: 0,
            last_solver_fallback: false,
            stats: [
                "updates",
                "detections",
                "created",
                "deleted",
                "assignments",
                "cross_slot",
                "recovered",
                "coasted_track_scans",
                "max_active",
                "gated_pairs",
                "distance_rejections",
                "speed_rejections",
                "bearing_rejections",
                "yaw_compensated_updates",
                "yaw_unavailable_updates",
                "ambiguous_identity_components",
                "ambiguous_identity_detections",
            ]
            .into_iter()
            .map(|s| (s.into(), 0))
            .collect(),
            trace_decisions: false,
            last_decisions: vec![],
            last_identity_evidence: BTreeMap::new(),
        })
    }
    pub fn update(
        &mut self,
        timestamp_ns: i64,
        detections: &[RawDetection],
        yaw_rate: Option<f64>,
    ) -> Result<Vec<Track>, String> {
        if timestamp_ns < 0 {
            return Err("timestamp_ns must be an integer in [0, None]".into());
        }
        if self
            .last_timestamp_ns
            .is_some_and(|last| timestamp_ns <= last)
        {
            return Err("scan timestamps must be strictly increasing".into());
        }
        if yaw_rate.is_some_and(|yaw| !yaw.is_finite()) {
            return Err("yaw_rate must be finite".into());
        }
        if detections.len() > 32 {
            return Err("a Bosch scan cannot contain more than 32 detections".into());
        }
        let mut slots = BTreeSet::new();
        for point in detections {
            if point.timestamp_ns > timestamp_ns {
                return Err("detection timestamp is later than scan availability".into());
            }
            if self
                .last_timestamp_ns
                .is_some_and(|last| point.timestamp_ns <= last)
            {
                return Err("stale/repeated detection cannot be tracked as fresh".into());
            }
            if !slots.insert(point.slot) {
                return Err("a scan must contain at most one detection per slot".into());
            }
        }
        let mut current = detections.to_vec();
        current.sort_by_key(|point| point.slot);
        let config = &self.config;
        let coast_ns = (config.coast_s * 1e9).round_ties_even() as i64;
        let retained: Vec<_> = self
            .states
            .values()
            .filter(|state| timestamp_ns - state.last_seen_scan_ns <= coast_ns)
            .cloned()
            .collect();
        let expired = self.states.len() - retained.len();
        let dt = self
            .last_timestamp_ns
            .map_or(0., |last| (timestamp_ns - last) as f64 / 1e9);
        let angle = yaw_rate.map_or(0., |yaw| yaw * dt);
        if !angle.is_finite() {
            return Err("math domain error".into());
        }
        let (cosine, sine) = (angle.cos(), angle.sin());
        let predicted: Vec<_> = retained
            .iter()
            .map(|state| {
                let x = state.x + state.track.detection.v_rel * dt;
                (cosine * x + sine * state.y, -sine * x + cosine * state.y)
            })
            .collect();
        if current.iter().any(|point| {
            yaw_rate.is_some_and(|yaw| {
                !(yaw * ((timestamp_ns - point.timestamp_ns) as f64 / 1e9)).is_finite()
            })
        }) {
            return Err("math domain error".into());
        }
        let observed: Vec<_> = current
            .iter()
            .map(|point| {
                let dt = (timestamp_ns - point.timestamp_ns) as f64 / 1e9;
                let angle = yaw_rate.map_or(0., |yaw| yaw * dt);
                let (cosine, sine) = (angle.cos(), angle.sin());
                let x = point.d_rel + point.v_rel * dt;
                (
                    cosine * x + sine * point.y_rel,
                    -sine * x + cosine * point.y_rel,
                )
            })
            .collect();
        let (n, m) = (retained.len(), current.len());
        let pred_bearing: Vec<_> = predicted.iter().map(|&(x, y)| y.atan2(x)).collect();
        let obs_bearing: Vec<_> = observed.iter().map(|&(x, y)| y.atan2(x)).collect();
        let (mut row_edges, mut column_edges) = (vec![vec![]; n], vec![vec![]; m]);
        let (mut distance_pairs, mut speed_rejections, mut bearing_rejections, mut gated_pairs) =
            (0, 0, 0, 0);
        let mut order: Vec<_> = (0..m).collect();
        order.sort_by(|&a, &b| observed[a].0.partial_cmp(&observed[b].0).unwrap());
        let mut rows: Vec<_> = (0..n).collect();
        rows.sort_by(|&a, &b| predicted[a].0.partial_cmp(&predicted[b].0).unwrap());
        let (mut lower, mut upper) = (0, 0);
        let window_gate = config.distance_gate_m.next_up();
        for row in rows {
            let pred_x = predicted[row].0;
            let point = &retained[row].track.detection;
            let minimum = (pred_x - window_gate).next_down();
            let maximum = (pred_x + window_gate).next_up();
            while lower < m && observed[order[lower]].0 < minimum {
                lower += 1;
            }
            upper = upper.max(lower);
            while upper < m && observed[order[upper]].0 <= maximum {
                upper += 1;
            }
            for &col in &order[lower..upper] {
                let obs_x = observed[col].0;
                let delta_d = (obs_x - pred_x).abs();
                if delta_d > config.distance_gate_m {
                    continue;
                }
                distance_pairs += 1;
                let delta_v = (current[col].v_rel - point.v_rel).abs();
                if delta_v > config.speed_gate_mps {
                    speed_rejections += 1;
                    continue;
                }
                let angle = obs_bearing[col] - pred_bearing[row];
                let delta_bearing = angle.sin().atan2(angle.cos()).abs();
                let nearest = if pred_x < obs_x { pred_x } else { obs_x };
                let gate = config.bearing_gate(if nearest < 0. { 0. } else { nearest });
                if delta_bearing > gate {
                    bearing_rejections += 1;
                } else {
                    gated_pairs += 1;
                    let mut cost = ((delta_d / config.distance_gate_m)
                        .powf(std::hint::black_box(2.))
                        + (delta_v / config.speed_gate_mps).powf(std::hint::black_box(2.))
                        + (delta_bearing / gate).powf(std::hint::black_box(2.)))
                        / 3.;
                    if point.slot == current[col].slot {
                        cost = (cost - config.same_slot_bonus).max(0.);
                    }
                    row_edges[row].push((col, cost));
                    column_edges[col].push((row, cost));
                }
            }
        }
        let assignment = graph::solve(n, m, &row_edges, &column_edges, config.unmatched_cost)
            .map_err(str::to_owned)?;
        let births = m - assignment.assignment.len();
        if u64::from(self.next_id) + births as u64 > i32::MAX as u64 + 1 {
            return Err(
                "raw return Int32 ID space exhausted; IDs must not wrap or be reused".into(),
            );
        }
        self.last_pair_possible = n * m;
        self.last_pair_candidates = distance_pairs;
        self.last_component_count = assignment.components;
        self.last_largest_component = assignment.largest;
        self.last_fast_components = assignment.certified;
        self.last_solver_fallback = assignment.fallback;
        self.last_tied_components = if assignment.fallback {
            0
        } else {
            assignment.components - assignment.certified
        };
        self.last_ambiguous_detections = assignment.ambiguous.len();
        self.update_count += 1;
        let claimed: BTreeSet<_> = assignment.assignment.values().copied().collect();
        self.states.clear();
        for (row, state) in retained.iter().enumerate() {
            if claimed.contains(&row) {
                continue;
            }
            let mut state = state.clone();
            (state.x, state.y) = predicted[row];
            self.states.insert(state.track.raw_track_id, state);
        }
        let (mut output, mut decisions, mut evidence) = (vec![], vec![], BTreeMap::new());
        let (mut assignments, mut cross_slot, mut recovered_count, mut created) = (0, 0, 0, 0);
        for (col, point) in current.iter().enumerate() {
            let row = assignment.assignment.get(&col).copied();
            let track = if let Some(row) = row {
                let old = &retained[row];
                let recovered = old.last_seen_update != self.update_count - 1;
                assignments += 1;
                cross_slot += usize::from(point.slot != old.track.detection.slot);
                recovered_count += usize::from(recovered);
                Track {
                    raw_track_id: old.track.raw_track_id,
                    detection: point.clone(),
                    age_scans: old.track.age_scans + 1,
                    recovered,
                    previous_slot: Some(old.track.detection.slot),
                }
            } else {
                let track = Track {
                    raw_track_id: self.next_id,
                    detection: point.clone(),
                    age_scans: 1,
                    recovered: false,
                    previous_slot: None,
                };
                self.next_id += 1;
                created += 1;
                track
            };
            let (x, y) = observed[col];
            self.states.insert(
                track.raw_track_id,
                State {
                    track: track.clone(),
                    x,
                    y,
                    last_seen_scan_ns: timestamp_ns,
                    last_seen_update: self.update_count,
                },
            );
            evidence.insert(
                track.raw_track_id,
                (
                    point.timestamp_ns,
                    column_edges[col].len(),
                    assignment.ambiguous.contains(&col),
                ),
            );
            if self.trace_decisions {
                let chosen = column_edges[col]
                    .iter()
                    .find(|(other, _)| Some(*other) == row)
                    .map(|(_, cost)| *cost);
                let mut alternatives: Vec<_> = column_edges[col]
                    .iter()
                    .filter(|(other, _)| Some(*other) != row)
                    .map(|(_, cost)| *cost)
                    .collect();
                alternatives.sort_by(f64::total_cmp);
                decisions.push(Decision {
                    timestamp_ns,
                    raw_track_id: track.raw_track_id,
                    slot: point.slot,
                    previous_slot: track.previous_slot,
                    created: row.is_none(),
                    recovered: track.recovered,
                    age_scans: track.age_scans,
                    d_rel: point.d_rel,
                    y_rel: point.y_rel,
                    v_rel: point.v_rel,
                    predicted_d_rel: row.map(|r| predicted[r].0),
                    predicted_y_rel: row.map(|r| predicted[r].1),
                    residual_d_m: row.map(|r| x - predicted[r].0),
                    residual_bearing_rad: row.map(|r| {
                        let a = obs_bearing[col] - pred_bearing[r];
                        a.sin().atan2(a.cos())
                    }),
                    bearing_gate_rad: row
                        .map(|r| config.bearing_gate(predicted[r].0.min(x).max(0.))),
                    chosen_cost: chosen,
                    best_alternative_cost: alternatives.first().copied(),
                    candidate_count: column_edges[col].len(),
                    identity_ambiguous: assignment.ambiguous.contains(&col),
                });
            }
            output.push(track);
        }
        self.last_decisions = decisions;
        self.last_identity_evidence = evidence;
        self.last_timestamp_ns = Some(timestamp_ns);
        for (key, value) in [
            ("gated_pairs", gated_pairs),
            ("distance_rejections", n * m - distance_pairs),
            ("speed_rejections", speed_rejections),
            ("bearing_rejections", bearing_rejections),
            ("assignments", assignments),
            ("cross_slot", cross_slot),
            ("recovered", recovered_count),
            ("created", created),
            ("updates", 1),
            ("detections", m),
            ("deleted", expired),
            ("coasted_track_scans", n - assignment.assignment.len()),
            ("ambiguous_identity_components", self.last_tied_components),
            ("ambiguous_identity_detections", assignment.ambiguous.len()),
            (
                if yaw_rate.is_some() {
                    "yaw_compensated_updates"
                } else {
                    "yaw_unavailable_updates"
                },
                1,
            ),
        ] {
            *self.stats.get_mut(key).unwrap() += value;
        }
        let max_active = self.stats.get_mut("max_active").unwrap();
        *max_active = (*max_active).max(self.states.len());
        Ok(output)
    }
}
