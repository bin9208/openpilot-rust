//! Persistent spatial-copy evidence; suppression never changes track identity.
use super::physical::Object;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Serialize, Deserialize)]
pub struct State {
    pub parent_pid: u32,
    pub since_ns: i64,
    pub last_ns: i64,
    pub samples: usize,
    pub candidate_raw_id: u32,
    pub parent_representative_raw_id: u32,
    pub d_offset_mean: f64,
    pub d_offset_m2: f64,
    pub y_offset_mean: f64,
    pub y_offset_m2: f64,
    pub dv_sq_sum: f64,
    pub anchor_candidate_y: f64,
    pub anchor_y_offset: f64,
    pub last_candidate_y: f64,
    pub last_d_offset: f64,
    pub last_y_offset: f64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct PersistentClone {
    pub mode: u8,
    #[serde(rename = "_states")]
    pub states: IndexMap<u32, State>,
    #[serde(rename = "_support_until")]
    pub support_until: BTreeMap<u32, i64>,
    pub last_ns: Option<i64>,
    pub would_suppress: BTreeSet<u32>,
    pub decisions: BTreeMap<u32, Value>,
    pub parent_pool_last: usize,
    pub parent_pool_peak: usize,
    pub pair_evaluations_last: usize,
    pub pair_evaluations_total: usize,
    pub pair_evaluations_peak: usize,
    pub state_peak: usize,
    pub publication_suppressed: usize,
}
impl PersistentClone {
    pub fn new(mode: u8) -> Result<Self, &'static str> {
        if mode > 2 {
            return Err("invalid Bosch Candidate P91 mode");
        }
        Ok(Self {
            mode,
            states: IndexMap::new(),
            support_until: BTreeMap::new(),
            last_ns: None,
            would_suppress: BTreeSet::new(),
            decisions: BTreeMap::new(),
            parent_pool_last: 0,
            parent_pool_peak: 0,
            pair_evaluations_last: 0,
            pair_evaluations_total: 0,
            pair_evaluations_peak: 0,
            state_peak: 0,
            publication_suppressed: 0,
        })
    }
    pub fn update(
        &mut self,
        objects: &[Object],
        timestamp: i64,
        ego: f64,
        word0: &BTreeSet<u32>,
        yaw: Option<f64>,
    ) -> BTreeSet<u32> {
        self.last_ns = Some(timestamp);
        self.would_suppress.clear();
        self.decisions.clear();
        self.pair_evaluations_last = 0;
        if self.mode == 0 || !ego.is_finite() || ego < 3. || objects.is_empty() {
            self.states.clear();
            self.support_until.clear();
            return self.would_suppress.clone();
        }
        let live: BTreeSet<_> = objects.iter().map(|o| o.physical_track_id).collect();
        self.support_until
            .retain(|pid, expiry| live.contains(pid) && *expiry >= timestamp);
        for obj in objects {
            if obj.vision_supported || obj.oem_selected || word0.contains(&obj.physical_track_id) {
                self.support_until
                    .insert(obj.physical_track_id, timestamp + 500_000_000);
            }
        }
        let supported: BTreeSet<_> = self
            .support_until
            .iter()
            .filter(|(_, expiry)| **expiry >= timestamp)
            .map(|(pid, _)| *pid)
            .collect();
        let parents: Vec<_> = objects
            .iter()
            .filter(|o| supported.contains(&o.physical_track_id))
            .collect();
        self.parent_pool_last = parents.len();
        self.parent_pool_peak = self.parent_pool_peak.max(parents.len());
        if parents.is_empty() {
            self.states.clear();
            return self.would_suppress.clone();
        }
        let rotation = yaw.filter(|v| v.is_finite()).unwrap_or(0.);
        let mut states = IndexMap::new();
        let mut suppress = BTreeSet::new();
        for candidate in objects {
            let pid = candidate.physical_track_id;
            if supported.contains(&pid) || candidate.members.len() != 1 {
                continue;
            }
            let world = candidate.v_rel + ego - rotation * candidate.y_rel;
            if !world.is_finite() || world < 4. {
                continue;
            }
            let previous = self.states.get(&pid);
            let mut plausible = vec![];
            for &parent in &parents {
                if parent.physical_track_id == pid {
                    continue;
                }
                self.pair_evaluations_last += 1;
                let d = candidate.d_rel - parent.d_rel;
                let y = candidate.y_rel - parent.y_rel;
                let dv = candidate.v_rel - parent.v_rel;
                if (6.0..=30.0).contains(&d) && y.abs() >= 3.5 && dv.abs() <= 0.75 {
                    plausible.push((parent, d, y, dv));
                }
            }
            let chosen = previous.and_then(|prior| {
                plausible
                    .iter()
                    .find(|(p, _, _, _)| p.physical_track_id == prior.parent_pid)
            });
            let Some(&(parent, d, y, dv)) = chosen.or_else(|| {
                plausible.iter().min_by(|(a, _, _, av), (b, _, _, bv)| {
                    av.abs()
                        .partial_cmp(&bv.abs())
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| b.age_scans.cmp(&a.age_scans))
                        .then_with(|| a.physical_track_id.cmp(&b.physical_track_id))
                })
            }) else {
                continue;
            };
            let member = candidate.members[0].raw_track_id;
            let continuous = previous.is_some_and(|p| {
                p.parent_pid == parent.physical_track_id
                    && timestamp - p.last_ns > 0
                    && timestamp - p.last_ns <= 320_000_000
                    && p.candidate_raw_id == member
                    && p.parent_representative_raw_id == parent.representative_raw_track_id
                    && (d - p.last_d_offset).abs() <= 1.5
                    && (y - p.last_y_offset).abs() <= 0.75
                    && (candidate.y_rel - p.last_candidate_y).abs() <= 0.75
                    && (candidate.y_rel - p.anchor_candidate_y).abs() <= 2.5
                    && (y - p.anchor_y_offset).abs() <= 2.5
            });
            let state = if continuous {
                let p = previous.unwrap();
                let samples = p.samples + 1;
                let (dm, dm2) = welford(p.d_offset_mean, p.d_offset_m2, samples, d);
                let (ym, ym2) = welford(p.y_offset_mean, p.y_offset_m2, samples, y);
                State {
                    parent_pid: parent.physical_track_id,
                    since_ns: p.since_ns,
                    last_ns: timestamp,
                    samples,
                    candidate_raw_id: member,
                    parent_representative_raw_id: parent.representative_raw_track_id,
                    d_offset_mean: dm,
                    d_offset_m2: dm2,
                    y_offset_mean: ym,
                    y_offset_m2: ym2,
                    dv_sq_sum: p.dv_sq_sum + dv * dv,
                    anchor_candidate_y: p.anchor_candidate_y,
                    anchor_y_offset: p.anchor_y_offset,
                    last_candidate_y: candidate.y_rel,
                    last_d_offset: d,
                    last_y_offset: y,
                }
            } else {
                State {
                    parent_pid: parent.physical_track_id,
                    since_ns: timestamp,
                    last_ns: timestamp,
                    samples: 1,
                    candidate_raw_id: member,
                    parent_representative_raw_id: parent.representative_raw_track_id,
                    d_offset_mean: d,
                    d_offset_m2: 0.,
                    y_offset_mean: y,
                    y_offset_m2: 0.,
                    dv_sq_sum: dv * dv,
                    anchor_candidate_y: candidate.y_rel,
                    anchor_y_offset: y,
                    last_candidate_y: candidate.y_rel,
                    last_d_offset: d,
                    last_y_offset: y,
                }
            };
            let age = timestamp - state.since_ns;
            let rmse = (state.dv_sq_sum / state.samples as f64).sqrt();
            let d_std = stddev(state.d_offset_m2, state.samples);
            let y_std = stddev(state.y_offset_m2, state.samples);
            let established = age >= 3_000_000_000
                && state.samples >= 25
                && rmse <= 0.35
                && d_std <= 0.75
                && y_std <= 0.80;
            if established {
                suppress.insert(pid);
            }
            self.decisions.insert(pid,json!({"would_suppress":established,"reason":if established{"persistent_spatial_copy"}else{"warming"},"parent_pid":parent.physical_track_id,"candidate_pid":pid,"evidence_age_s":age as f64*1e-9,"samples":state.samples,"v_rmse_mps":rmse,"d_offset_mean_m":state.d_offset_mean,"d_offset_std_m":d_std,"y_offset_mean_m":state.y_offset_mean,"y_offset_std_m":y_std,"world_speed_mps":world}));
            states.insert(pid, state);
        }
        let parent_ids: BTreeSet<_> = parents.iter().map(|o| o.physical_track_id).collect();
        for (&pid, state) in &self.states {
            if !live.contains(&pid)
                && parent_ids.contains(&state.parent_pid)
                && timestamp - state.last_ns <= 320_000_000
            {
                states.insert(pid, state.clone());
            }
        }
        self.states = states;
        self.would_suppress = suppress;
        self.pair_evaluations_total += self.pair_evaluations_last;
        self.pair_evaluations_peak = self.pair_evaluations_peak.max(self.pair_evaluations_last);
        self.state_peak = self.state_peak.max(self.states.len());
        self.would_suppress.clone()
    }
}
fn welford(mut mean: f64, m2: f64, samples: usize, value: f64) -> (f64, f64) {
    let delta = value - mean;
    mean += delta / samples as f64;
    (mean, m2 + delta * (value - mean))
}
fn stddev(m2: f64, samples: usize) -> f64 {
    if samples == 0 {
        f64::INFINITY
    } else {
        let value = m2 / samples as f64;
        let clipped = if value > 0. { value } else { 0. };
        clipped.sqrt()
    }
}
