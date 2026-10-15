//! Sliding-scatterer lateral measurement gating; identity anchors never change.
use super::{
    oem::float_value,
    physical::{Object, Surface},
    retention::path_offset,
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
#[derive(Clone, Serialize, Deserialize)]
pub struct State {
    pub raw_track_id: u32,
    pub samples: VecDeque<(i64, f64, f64)>,
    pub armed: bool,
    pub anchor_y_m: f64,
    pub offset_m: f64,
    pub quiet_scans: usize,
    pub last_ns: i64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct SidePass {
    pub mode: u8,
    #[serde(rename = "_states")]
    pub states: IndexMap<u32, State>,
    pub last_ns: Option<i64>,
    pub published_y: BTreeMap<u32, f64>,
    pub last_decisions: Vec<Value>,
    pub arms: usize,
    pub releases: usize,
    pub publication_adjusted: usize,
    pub state_peak: usize,
}
impl SidePass {
    pub fn new(mode: u8) -> Result<Self, &'static str> {
        if mode > 2 {
            return Err("invalid Bosch side-pass lateral mode");
        }
        Ok(Self {
            mode,
            states: IndexMap::new(),
            last_ns: None,
            published_y: BTreeMap::new(),
            last_decisions: vec![],
            arms: 0,
            releases: 0,
            publication_adjusted: 0,
            state_peak: 0,
        })
    }
    pub fn reset(&mut self, timestamp: i64, reason: &str) {
        let mut states: Vec<_> = self
            .states
            .iter()
            .filter(|(_, s)| s.armed || s.offset_m != 0.)
            .collect();
        states.sort_by_key(|(pid, _)| **pid);
        self.last_decisions = states
            .into_iter()
            .map(|(pid, _)| {
                decision(
                    timestamp,
                    *pid,
                    "RESET",
                    reason,
                    f64::NAN,
                    f64::NAN,
                    f64::NAN,
                )
            })
            .collect();
        self.states.clear();
        self.published_y.clear();
        self.last_ns = Some(timestamp);
    }
    pub fn update(
        &mut self,
        objects: &[Object],
        timestamp: i64,
        ego: f64,
        path: &[(f64, f64)],
        oem: &BTreeSet<u32>,
    ) -> Result<BTreeMap<u32, f64>, &'static str> {
        self.last_decisions.clear();
        if self.mode == 0 {
            self.states.clear();
            self.published_y.clear();
            self.last_ns = Some(timestamp);
            return Ok(self.published_y.clone());
        }
        let mut decisions = vec![];
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
                decisions.extend(self.last_decisions.clone());
            }
        }
        self.last_ns = Some(timestamp);
        let speed_known = ego.is_finite();
        let mut published = BTreeMap::new();
        let mut live = BTreeSet::new();
        for obj in objects {
            let pid = obj.physical_track_id;
            let rep = obj.representative_raw_track_id;
            let reachable = obj.v_rel < -0.5
                && speed_known
                && obj.v_rel + ego >= 1.
                && obj.d_rel <= 50.
                && obj.y_rel.abs() <= 6.9;
            let changed = self.states.get(&pid).is_some_and(|s| s.raw_track_id != rep);
            if !self.states.contains_key(&pid) || changed {
                if let Some(state) = self.states.shift_remove(&pid) {
                    if state.armed || state.offset_m != 0. {
                        decisions.push(decision(
                            timestamp,
                            pid,
                            "RESET",
                            "REPRESENTATIVE_CHANGE",
                            f64::NAN,
                            obj.y_rel,
                            obj.y_rel,
                        ));
                    }
                }
                if obj.members.len() != 1 || !reachable {
                    continue;
                }
                self.states.insert(
                    pid,
                    State {
                        raw_track_id: rep,
                        samples: VecDeque::new(),
                        armed: false,
                        anchor_y_m: 0.,
                        offset_m: 0.,
                        quiet_scans: 0,
                        last_ns: 0,
                    },
                );
            } else if self
                .states
                .get(&pid)
                .is_some_and(|s| !reachable && !s.armed && s.offset_m == 0.)
            {
                self.states.shift_remove(&pid);
                continue;
            }
            let member = if obj.members.len() == 1 {
                &obj.members[0]
            } else {
                obj.members
                    .iter()
                    .find(|m| m.raw_track_id == rep)
                    .ok_or("representative raw member missing")?
            };
            let state = self.states.get_mut(&pid).unwrap();
            state
                .samples
                .push_back((member.detection.timestamp_ns, obj.d_rel, obj.v_rel));
            while state
                .samples
                .front()
                .is_some_and(|s| timestamp - s.0 > 800_000_000)
            {
                state.samples.pop_front();
            }
            if state.samples.is_empty() {
                return Err("deque index out of range");
            }
            state.last_ns = timestamp;
            live.insert(pid);
            let slide = slide(&state.samples);
            let y = obj.y_rel;
            let published_y = y + state.offset_m;
            let closing = obj.v_rel <= -1.5;
            let moving = speed_known && obj.v_rel + ego >= 3.;
            let armable = slide.is_some_and(|s| s >= 1.5)
                && obj.members.len() == 1
                && closing
                && moving
                && obj.d_rel <= 45.;
            if !(state.armed || state.offset_m != 0. || armable) {
                continue;
            }
            let offset = path_offset(obj.d_rel, obj.y_rel, path);
            let release = if offset.abs() < 1.9 {
                Some("CORRIDOR_ENTRY")
            } else if oem.contains(&pid) || obj.oem_selected {
                Some("OEM_SELECTED")
            } else if state.offset_m.abs() > 1. {
                Some("BEYOND_BODY")
            } else {
                None
            };
            if let Some(release) = release {
                if state.armed || state.offset_m != 0. {
                    decisions.push(decision(
                        timestamp,
                        pid,
                        "RELEASE",
                        release,
                        slide.unwrap_or(f64::NAN),
                        y,
                        y,
                    ));
                    self.releases += 1;
                }
                state.armed = false;
                state.offset_m = 0.;
                state.quiet_scans = 0;
                continue;
            }
            let in_zone = offset.abs() <= 5.4 && obj.d_rel <= 45.;
            if !state.armed {
                if armable && in_zone {
                    state.armed = true;
                    state.anchor_y_m = published_y;
                    state.quiet_scans = 0;
                    self.arms += 1;
                    decisions.push(decision(
                        timestamp,
                        pid,
                        "ARM",
                        "SLIDE",
                        slide.unwrap(),
                        y,
                        published_y,
                    ));
                }
            } else {
                let quiet = slide.is_none_or(|s| s < 0.5)
                    || !closing
                    || !moving
                    || !in_zone
                    || obj.members.len() != 1;
                state.quiet_scans = if quiet { state.quiet_scans + 1 } else { 0 };
                if state.quiet_scans >= 3 {
                    state.armed = false;
                    self.releases += 1;
                    decisions.push(decision(
                        timestamp,
                        pid,
                        "RELEASE",
                        "EVIDENCE_END",
                        slide.unwrap_or(f64::NAN),
                        y,
                        published_y,
                    ));
                }
            }
            if state.armed {
                state.offset_m = state.anchor_y_m - y;
                if state.offset_m.abs() > 1. {
                    state.armed = false;
                    state.offset_m = 0.;
                    self.releases += 1;
                    decisions.push(decision(
                        timestamp,
                        pid,
                        "RELEASE",
                        "BEYOND_BODY",
                        slide.unwrap_or(f64::NAN),
                        y,
                        y,
                    ));
                }
            } else if state.offset_m != 0. {
                let step = 0.10 * 100_000_000_f64 * 1e-9;
                if state.offset_m.abs() <= step {
                    state.offset_m = 0.;
                    decisions.push(decision(
                        timestamp,
                        pid,
                        "REALIGNED",
                        "",
                        slide.unwrap_or(f64::NAN),
                        y,
                        y,
                    ));
                } else {
                    state.offset_m -= step.copysign(state.offset_m);
                }
            }
            if state.offset_m != 0. {
                published.insert(pid, y + state.offset_m);
            }
        }
        let mut absent = vec![];
        for (&pid, state) in &self.states {
            if live.contains(&pid) {
                continue;
            }
            if state.armed || state.offset_m != 0. {
                if timestamp - state.last_ns <= 320_000_000 {
                    continue;
                }
                decisions.push(decision(
                    timestamp,
                    pid,
                    "RESET",
                    "ABSENT",
                    f64::NAN,
                    f64::NAN,
                    f64::NAN,
                ));
            }
            absent.push(pid);
        }
        for pid in absent {
            self.states.shift_remove(&pid);
        }
        if self.states.len() > 64 {
            let mut victims: Vec<_> = self
                .states
                .iter()
                .map(|(pid, s)| (*pid, s.armed || s.offset_m != 0., s.last_ns))
                .collect();
            victims.sort_by_key(|(pid, anchored, ns)| (*anchored, *ns, *pid));
            for (pid, _, _) in victims.into_iter().take(self.states.len() - 64) {
                self.states.shift_remove(&pid);
                published.remove(&pid);
            }
        }
        self.state_peak = self.state_peak.max(self.states.len());
        self.published_y = published;
        self.last_decisions = decisions;
        Ok(self.published_y.clone())
    }
    pub fn publication_view(&mut self, objects: &[Object]) -> Vec<Object> {
        if self.mode != 2 || self.published_y.is_empty() || objects.is_empty() {
            return objects.to_vec();
        }
        objects
            .iter()
            .map(|obj| {
                let (Some(&y), Some(state)) = (
                    self.published_y.get(&obj.physical_track_id),
                    self.states.get(&obj.physical_track_id),
                ) else {
                    return obj.clone();
                };
                if Some(obj.timestamp_ns) != self.last_ns
                    || obj.published_surface.is_some()
                    || obj.representative_raw_track_id != state.raw_track_id
                {
                    return obj.clone();
                }
                let mut adjusted = obj.clone();
                adjusted.published_surface = Some(Surface {
                    raw_track_id: state.raw_track_id,
                    d_rel: obj.d_rel,
                    y_rel: y,
                    v_rel: obj.v_rel,
                    lateral_estimate: true,
                });
                self.publication_adjusted += 1;
                adjusted
            })
            .collect()
    }
}
fn slide(samples: &VecDeque<(i64, f64, f64)>) -> Option<f64> {
    let n = samples.len();
    if n < 5 || samples.back()?.0 - samples.front()?.0 < 400_000_000 {
        return None;
    }
    let t0 = samples.front()?.0;
    let (mut st, mut sd, mut stt, mut std, mut sv) = (0., 0., 0., 0., 0.);
    for &(ns, d, v) in samples {
        let t = (ns - t0) as f64 * 1e-9;
        st += t;
        sd += d;
        stt += t * t;
        std += t * d;
        sv += v;
    }
    let n = n as f64;
    let denominator = n * stt - st * st;
    if denominator <= 0. {
        None
    } else {
        Some((n * std - st * sd) / denominator - sv / n)
    }
}
fn decision(
    timestamp: i64,
    pid: u32,
    action: &str,
    reason: &str,
    slide: f64,
    raw: f64,
    published: f64,
) -> Value {
    json!({"timestamp_ns":timestamp,"physical_track_id":pid,"action":action,"reason":reason,"slide_mps":float_value(slide),"raw_y_m":float_value(raw),"published_y_m":float_value(published)})
}
