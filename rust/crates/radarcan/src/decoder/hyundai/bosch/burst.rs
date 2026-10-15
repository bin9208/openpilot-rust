//! Present-scan burst proof with bounded publication-only deferral.
use super::physical::Object;
use crate::scalar::float_sum;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Serialize, Deserialize)]
pub struct State {
    pub child_pid: u32,
    pub anchor_pid: u32,
    pub arm_ns: i64,
    pub last_ns: i64,
    pub burst_members: usize,
    pub burst_d_span_m: f64,
    pub burst_y_span_m: f64,
    pub burst_world_mean_mps: f64,
    pub child_d_rel_m: f64,
    pub child_y_rel_m: f64,
    pub anchor_age: usize,
    pub held_scans: usize,
    pub missing_scans: usize,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Burst {
    pub mode: u8,
    #[serde(rename = "_states")]
    pub states: IndexMap<u32, State>,
    pub last_ns: Option<i64>,
    pub would_suppress: BTreeSet<u32>,
    pub last_decisions: Vec<Value>,
    pub pair_evaluations_last: usize,
    pub pair_evaluations_total: usize,
    pub pair_evaluations_peak: usize,
    pub holds: usize,
    pub releases: usize,
    pub publication_suppressed: usize,
    pub state_peak: usize,
}
impl Burst {
    pub fn new(mode: u8) -> Result<Self, &'static str> {
        if mode > 2 {
            return Err("invalid Bosch burst multi-return mode");
        }
        Ok(Self {
            mode,
            states: IndexMap::new(),
            last_ns: None,
            would_suppress: BTreeSet::new(),
            last_decisions: vec![],
            pair_evaluations_last: 0,
            pair_evaluations_total: 0,
            pair_evaluations_peak: 0,
            holds: 0,
            releases: 0,
            publication_suppressed: 0,
            state_peak: 0,
        })
    }
    pub fn reset(&mut self, timestamp: i64, reason: &str) {
        self.last_decisions = self
            .states
            .values()
            .map(|s| decision(timestamp, "RELEASE", reason, s))
            .collect();
        self.releases += self.last_decisions.len();
        self.states.clear();
        self.would_suppress.clear();
        self.last_ns = Some(timestamp);
    }
    pub fn update(
        &mut self,
        objects: &[Object],
        timestamp: i64,
        ego: f64,
        yaw: Option<f64>,
        validated: &BTreeSet<u32>,
    ) -> BTreeSet<u32> {
        self.would_suppress.clear();
        self.last_decisions.clear();
        self.pair_evaluations_last = 0;
        if self.mode == 0 {
            self.states.clear();
            self.last_ns = Some(timestamp);
            return self.would_suppress.clone();
        }
        if !ego.is_finite() {
            self.reset(timestamp, "SPEED_DIVERGENCE");
            return self.would_suppress.clone();
        }
        if let Some(last) = self.last_ns {
            if timestamp <= last || timestamp - last > 320_000_000 {
                self.reset(
                    timestamp,
                    if timestamp <= last {
                        "STATE_RESET"
                    } else {
                        "SCAN_GAP"
                    },
                );
                return self.would_suppress.clone();
            }
        }
        self.last_ns = Some(timestamp);
        let present: BTreeMap<_, _> = objects.iter().map(|o| (o.physical_track_id, o)).collect();
        let mut decisions = vec![];
        let mut suppress = BTreeSet::new();
        let mut alive = IndexMap::new();
        for (&pid, prior) in &self.states {
            let mut state = prior.clone();
            let child = present.get(&pid).copied();
            let reason = if let Some(child) = child {
                if child.oem_selected || child.vision_supported || validated.contains(&pid) {
                    "CHILD_EVIDENCE"
                } else if child.members.len() != 1 {
                    "CHILD_GREW"
                } else if state.held_scans >= 5 {
                    "HOLD_BUDGET"
                } else {
                    state.held_scans += 1;
                    state.missing_scans = 0;
                    state.last_ns = timestamp;
                    alive.insert(pid, state);
                    suppress.insert(pid);
                    continue;
                }
            } else if state.missing_scans < 1 {
                state.missing_scans += 1;
                alive.insert(pid, state);
                suppress.insert(pid);
                continue;
            } else {
                "CHILD_LOST"
            };
            decisions.push(decision(timestamp, "RELEASE", reason, &state));
            self.releases += 1;
        }
        self.states = alive;
        if ego >= 8. {
            let yaw = yaw.filter(|v| v.is_finite()).unwrap_or(0.);
            let world = |o: &Object| o.v_rel + ego - yaw * o.y_rel;
            let mut newborn: Vec<_> = objects
                .iter()
                .filter(|o| o.age_scans == 1 && o.members.len() == 1)
                .map(|o| (world(o), o))
                .collect();
            if newborn.len() >= 3 {
                newborn.sort_by(|(a, x), (b, y)| {
                    cmp(*a, *b)
                        .then_with(|| cmp(x.d_rel, y.d_rel))
                        .then_with(|| cmp(x.y_rel, y.y_rel))
                        .then_with(|| x.physical_track_id.cmp(&y.physical_track_id))
                });
                let mut others = None;
                let mut index = 0;
                while index < newborn.len() {
                    let mut end = index;
                    while end + 1 < newborn.len() && newborn[end + 1].0 - newborn[index].0 <= 0.35 {
                        end += 1;
                    }
                    let group = &newborn[index..=end];
                    index = end + 1;
                    if group.len() < 3 {
                        continue;
                    }
                    let ranges: Vec<_> = group.iter().map(|(_, o)| o.d_rel).collect();
                    let offsets: Vec<_> = group.iter().map(|(_, o)| o.y_rel).collect();
                    let d_span = span(&ranges);
                    let y_span = span(&offsets);
                    if d_span > 3. || y_span < 8. {
                        continue;
                    }
                    let mean = float_sum(group.iter().map(|(v, _)| *v)) / group.len() as f64;
                    if mean.abs() < 4. {
                        continue;
                    }
                    let others = others.get_or_insert_with(|| {
                        objects
                            .iter()
                            .filter(|o| o.age_scans >= 25)
                            .map(|o| (world(o), o))
                            .collect::<Vec<_>>()
                    });
                    let members: BTreeSet<_> =
                        group.iter().map(|(_, o)| o.physical_track_id).collect();
                    let mut neighbours = vec![];
                    for &(speed, obj) in others.iter() {
                        if members.contains(&obj.physical_track_id) {
                            continue;
                        }
                        self.pair_evaluations_last += 1;
                        if (speed - mean).abs() <= 0.35
                            && ranges
                                .iter()
                                .map(|v| (obj.d_rel - v).abs())
                                .reduce(f64::min)
                                .unwrap()
                                <= 30.
                        {
                            neighbours.push(obj);
                        }
                    }
                    let Some(anchor) = neighbours.into_iter().min_by(|a, b| {
                        b.age_scans
                            .cmp(&a.age_scans)
                            .then_with(|| cmp(a.d_rel, b.d_rel))
                            .then_with(|| a.physical_track_id.cmp(&b.physical_track_id))
                    }) else {
                        continue;
                    };
                    for &(_, child) in group {
                        let pid = child.physical_track_id;
                        if self.states.contains_key(&pid)
                            || child.oem_selected
                            || child.vision_supported
                            || validated.contains(&pid)
                        {
                            continue;
                        }
                        if self.states.len() >= 16 {
                            break;
                        }
                        let state = State {
                            child_pid: pid,
                            anchor_pid: anchor.physical_track_id,
                            arm_ns: timestamp,
                            last_ns: timestamp,
                            burst_members: group.len(),
                            burst_d_span_m: d_span,
                            burst_y_span_m: y_span,
                            burst_world_mean_mps: mean,
                            child_d_rel_m: child.d_rel,
                            child_y_rel_m: child.y_rel,
                            anchor_age: anchor.age_scans,
                            held_scans: 1,
                            missing_scans: 0,
                        };
                        decisions.push(decision(timestamp, "HOLD", "", &state));
                        self.states.insert(pid, state);
                        suppress.insert(pid);
                        self.holds += 1;
                    }
                }
            }
        }
        self.would_suppress = suppress;
        self.last_decisions = decisions;
        self.pair_evaluations_total += self.pair_evaluations_last;
        self.pair_evaluations_peak = self.pair_evaluations_peak.max(self.pair_evaluations_last);
        self.state_peak = self.state_peak.max(self.states.len());
        self.would_suppress.clone()
    }
    pub fn publication_view(&mut self, objects: &[Object]) -> Vec<Object> {
        if self.mode != 2 || self.would_suppress.is_empty() || objects.is_empty() {
            return objects.to_vec();
        }
        let suppressed: BTreeSet<_> = objects
            .iter()
            .filter(|o| self.would_suppress.contains(&o.physical_track_id))
            .map(|o| o.physical_track_id)
            .collect();
        self.publication_suppressed += suppressed.len();
        objects
            .iter()
            .filter(|o| !suppressed.contains(&o.physical_track_id))
            .cloned()
            .collect()
    }
}
fn cmp(a: f64, b: f64) -> std::cmp::Ordering {
    a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
}
fn span(values: &[f64]) -> f64 {
    values.iter().copied().reduce(f64::max).unwrap()
        - values.iter().copied().reduce(f64::min).unwrap()
}
fn decision(timestamp: i64, action: &str, reason: &str, s: &State) -> Value {
    json!({"timestamp_ns":timestamp,"action":action,"release_reason":reason,"child_pid":s.child_pid,"anchor_pid":s.anchor_pid,"burst_members":s.burst_members,"burst_d_span_m":s.burst_d_span_m,"burst_y_span_m":s.burst_y_span_m,"burst_world_mean_mps":s.burst_world_mean_mps,"child_d_rel_m":s.child_d_rel_m,"child_y_rel_m":s.child_y_rel_m,"anchor_age":s.anchor_age,"held_scans":s.held_scans})
}
