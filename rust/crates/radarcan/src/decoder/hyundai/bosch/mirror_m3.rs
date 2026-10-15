//! Independent M3 edge-birth and bounded coupled-extension publication hold.
use super::{
    birth_b5::{Model, Number},
    physical::Object,
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
pub type MirrorObject = (f64, f64, f64, usize, Vec<u32>, Option<u32>, bool, bool);
pub type EdgeSample = (i64, Vec<Number>, Vec<Number>, Vec<Number>);
#[derive(Clone, Serialize, Deserialize)]
pub struct Hold {
    pub physical_track_id: u32,
    pub path: String,
    pub parent_pid: u32,
    pub wall_y_m: f64,
    pub residual_m: f64,
    pub d_rel_m: f64,
    pub y_rel_m: f64,
    pub v_rel_mps: f64,
    pub v_ego_mps: f64,
    pub birth_ns: i64,
    pub held_scans: usize,
    pub parent_miss_scans: usize,
    pub diverge_scans: usize,
    pub extension_scans: usize,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct MirrorM3 {
    pub mode: u8,
    pub model_edges: Vec<EdgeSample>,
    pub holds: IndexMap<u32, Hold>,
    pub seen_births: BTreeSet<u32>,
    pub last_ns: Option<i64>,
    pub last_edge_sample: Option<EdgeSample>,
    pub last_events: Vec<Value>,
    pub publication_suppressed: usize,
    pub last_publication_suppressed: BTreeSet<u32>,
    pub reset_count: usize,
    #[serde(rename = "_pending_events", skip_serializing_if = "Option::is_none")]
    pub pending_events: Option<Vec<Value>>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct BirthDecision {
    pub action: String,
    #[serde(default)]
    pub parent_pid: Option<u32>,
    #[serde(default)]
    pub wall_y_m: Option<f64>,
    #[serde(default)]
    pub residual_m: Option<f64>,
}
pub struct Frame<'a> {
    pub timestamp: i64,
    pub ego: f64,
    pub mirror: &'a IndexMap<u32, MirrorObject>,
    pub decisions: &'a IndexMap<u32, BirthDecision>,
    pub camera: &'a BTreeMap<u32, (i32, i64)>,
    pub word0: &'a BTreeSet<u32>,
    pub mirror_enabled: bool,
    pub live: Option<&'a BTreeSet<u32>>,
}
impl MirrorM3 {
    pub fn new(mode: u8) -> Result<Self, &'static str> {
        if mode > 2 {
            return Err("invalid Bosch mirror M3 mode");
        }
        Ok(Self {
            mode,
            model_edges: vec![],
            holds: IndexMap::new(),
            seen_births: BTreeSet::new(),
            last_ns: None,
            last_edge_sample: None,
            last_events: vec![],
            publication_suppressed: 0,
            last_publication_suppressed: BTreeSet::new(),
            reset_count: 0,
            pending_events: None,
        })
    }
    pub fn ingest_model(&mut self, model: Option<&Model>, timestamp: i64) {
        let Some(model) = model else { return };
        if self.mode == 0
            || model.edges.len() < 2
            || self.model_edges.iter().any(|s| s.0 == timestamp)
        {
            return;
        }
        let sample = (
            timestamp,
            model.edges[0].x.clone(),
            model.edges[0].y.iter().map(|v| Number(-v.0)).collect(),
            model.edges[1].y.iter().map(|v| Number(-v.0)).collect(),
        );
        self.model_edges.push(sample);
        self.model_edges.sort_by_key(|s| s.0);
        if self.model_edges.len() > 4 {
            self.model_edges.drain(..self.model_edges.len() - 4);
        }
    }
    fn edge_at(&self, timestamp: i64) -> Option<&EdgeSample> {
        self.model_edges
            .iter()
            .rev()
            .find(|s| s.0 <= timestamp)
            .filter(|s| timestamp - s.0 <= 200_000_000)
    }
    pub fn reset(&mut self, timestamp: Option<i64>, reason: &str) {
        let timestamp = timestamp.or(self.last_ns);
        self.last_events = timestamp
            .map(|ns| {
                self.holds
                    .values()
                    .map(|s| event(s, "RELEASE", reason, ns))
                    .collect()
            })
            .unwrap_or_default();
        self.holds.clear();
        self.seen_births.clear();
        self.last_ns = None;
        self.last_edge_sample = None;
        self.reset_count += 1;
        self.pending_events = None;
    }
    pub fn edge_birth(
        pid: u32,
        obj: &MirrorObject,
        mirror: &IndexMap<u32, MirrorObject>,
        edge: Option<&EdgeSample>,
        ego: f64,
    ) -> Option<(u32, f64, f64)> {
        let edge = edge?;
        if !ego.is_finite() || ego < 10. || obj.0 > 80. {
            return None;
        }
        let side = if obj.1 > 0. { 1. } else { -1. };
        if side < 0. {
            return None;
        }
        let xs = &edge.1;
        let ys = if side > 0. { &edge.2 } else { &edge.3 };
        if xs.is_empty() || xs.len() != ys.len() {
            return None;
        }
        let wall = interp(xs, ys, obj.0 + 1.52);
        if !wall.is_finite() || side * wall <= 0.5 || side * obj.1 < side * wall + 1. {
            return None;
        }
        let mut best = None;
        for (&parent_pid, parent) in mirror {
            if parent_pid == pid
                || parent.3 < 10
                || (parent.2 + ego).abs() < 2.
                || side * parent.1 > side * wall - 1.
                || (obj.0 - parent.0).abs() > 1.5
                || (obj.2 - parent.2).abs() > 0.5
            {
                continue;
            }
            let residual = (obj.1 - (2. * wall - parent.1)).abs();
            if residual <= 2. && best.is_none_or(|v: (u32, f64, f64)| residual < v.2) {
                best = Some((parent_pid, wall, residual));
            }
        }
        best
    }
    pub fn update(&mut self, f: Frame<'_>) -> BTreeSet<u32> {
        let mut events = vec![];
        self.pending_events = Some(vec![]);
        self.last_edge_sample = None;
        if self.mode == 0 || !f.mirror_enabled {
            if !self.holds.is_empty() || !self.seen_births.is_empty() {
                self.reset(
                    Some(f.timestamp),
                    if self.mode == 0 { "M3_OFF" } else { "M1_OFF" },
                );
            } else {
                self.last_events.clear();
            }
            return BTreeSet::new();
        }
        let edge = self.edge_at(f.timestamp).cloned();
        self.last_edge_sample = edge.clone();
        if self
            .last_ns
            .is_some_and(|last| f.timestamp - last <= 0 || f.timestamp - last >= 300_000_000)
        {
            events.extend(
                self.holds
                    .values()
                    .map(|s| event(s, "RELEASE", "GAP_RESET", f.timestamp)),
            );
            self.holds.clear();
            self.seen_births.clear();
            self.reset_count += 1;
        }
        self.last_ns = Some(f.timestamp);
        if let Some(live) = f.live {
            self.seen_births.retain(|id| live.contains(id));
        }
        let mut removed = vec![];
        for (&pid, s) in &mut self.holds {
            let Some(obj) = f.mirror.get(&pid) else {
                events.push(event(s, "RELEASE", "PID_DEAD", f.timestamp));
                removed.push(pid);
                continue;
            };
            s.held_scans += 1;
            let parent = f.mirror.get(&s.parent_pid);
            let side = if s.wall_y_m > 0. { 1. } else { -1. };
            let mut reason = None;
            if independent(pid, obj, &f) {
                reason = Some("INDEPENDENT_IDENTITY")
            } else if side * obj.1 < side * s.wall_y_m + 0.5 || obj.1.abs() < 4.5 {
                reason = Some("MOVED_INSIDE")
            } else {
                s.parent_miss_scans = if parent.is_none() {
                    s.parent_miss_scans + 1
                } else {
                    0
                };
                if s.parent_miss_scans >= 2 {
                    reason = Some("PARENT_LOST")
                } else if let Some(parent) = parent {
                    let diverged = (obj.2 - parent.2).abs() > 1.5 || (obj.0 - parent.0).abs() > 5.;
                    s.diverge_scans = if diverged { s.diverge_scans + 1 } else { 0 };
                    if s.diverge_scans >= 3 {
                        reason = Some("DIVERGED")
                    }
                }
            }
            if reason.is_none() && s.held_scans > 10 {
                let coupled = parent.is_some_and(|p| {
                    (obj.0 - p.0).abs() <= 3.
                        && (obj.2 - p.2).abs() <= 1.
                        && (obj.1 - (2. * s.wall_y_m - p.1)).abs() <= 3.
                });
                if !coupled {
                    reason = Some("EXPIRED")
                } else if s.held_scans > 50 {
                    reason = Some("EXT_CAP")
                } else {
                    s.extension_scans += 1
                }
            }
            if let Some(reason) = reason {
                events.push(event(s, "RELEASE", reason, f.timestamp));
                removed.push(pid);
            }
        }
        for pid in removed {
            self.holds.shift_remove(&pid);
        }
        for (&pid, obj) in f.mirror {
            if obj.3 != 1 || self.seen_births.contains(&pid) {
                continue;
            }
            self.seen_births.insert(pid);
            if obj.1.abs() < 4.5 {
                continue;
            }
            let proof = if let Some(d) = f.decisions.get(&pid).filter(|d| d.action == "QUALIFY") {
                Some((
                    "V1",
                    d.parent_pid.unwrap(),
                    d.wall_y_m.unwrap(),
                    d.residual_m.unwrap(),
                ))
            } else if !independent(pid, obj, &f) {
                Self::edge_birth(pid, obj, f.mirror, edge.as_ref(), f.ego)
                    .map(|(p, w, r)| ("EDGE", p, w, r))
            } else {
                None
            };
            let Some((path, parent, wall, residual)) = proof else {
                continue;
            };
            let state = Hold {
                physical_track_id: pid,
                path: path.into(),
                parent_pid: parent,
                wall_y_m: round_three(wall),
                residual_m: round_three(residual),
                d_rel_m: obj.0,
                y_rel_m: obj.1,
                v_rel_mps: obj.2,
                v_ego_mps: f.ego,
                birth_ns: f.timestamp,
                held_scans: 1,
                parent_miss_scans: 0,
                diverge_scans: 0,
                extension_scans: 0,
            };
            events.push(event(&state, "HOLD", "MIRROR_BIRTH_QUALIFIED", f.timestamp));
            self.holds.insert(pid, state);
        }
        self.last_events = events;
        self.pending_events = None;
        self.holds.keys().copied().collect()
    }
    pub fn publication_view(&mut self, objects: &[Object]) -> Vec<Object> {
        self.publication_by(objects, |o| o.physical_track_id)
    }
    pub fn publication_by<T: Clone>(&mut self, objects: &[T], id: impl Fn(&T) -> u32) -> Vec<T> {
        self.last_publication_suppressed.clear();
        if self.mode != 2 || objects.is_empty() || self.holds.is_empty() {
            return objects.to_vec();
        }
        let suppressed: BTreeSet<_> = objects
            .iter()
            .map(&id)
            .filter(|id| self.holds.contains_key(id))
            .collect();
        if suppressed.is_empty() {
            return objects.to_vec();
        }
        self.publication_suppressed += suppressed.len();
        self.last_publication_suppressed = suppressed;
        objects
            .iter()
            .filter(|o| !self.last_publication_suppressed.contains(&id(o)))
            .cloned()
            .collect()
    }
}
fn independent(pid: u32, obj: &MirrorObject, f: &Frame<'_>) -> bool {
    obj.7
        || obj.6
        || f.word0.contains(&pid)
        || f.camera
            .get(&pid)
            .is_some_and(|&(state, episode)| state == 1 && episode >= 0)
}
fn event(s: &Hold, action: &str, reason: &str, timestamp: i64) -> Value {
    let mut v = json!({"action":action,"ns":timestamp,"pid":s.physical_track_id,"birth_ns":s.birth_ns,"path":s.path,"parent":s.parent_pid,"wall_y":s.wall_y_m,"resid":s.residual_m,"d":s.d_rel_m,"y":s.y_rel_m,"v":s.v_rel_mps,"v_ego":s.v_ego_mps});
    if action == "RELEASE" {
        let obj = v.as_object_mut().unwrap();
        obj.insert("reason".into(), json!(reason));
        obj.insert("held_scans".into(), json!(s.held_scans));
        obj.insert("ext_scans".into(), json!(s.extension_scans));
    }
    v
}
pub fn round_three(v: f64) -> f64 {
    if !v.is_finite() {
        v
    } else {
        format!("{v:.3}").parse().expect("formatted finite decimal")
    }
}
pub(super) fn interp(xs: &[Number], ys: &[Number], x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    let n = xs.len();
    if x > xs[n - 1].0 {
        return ys[n - 1].0;
    }
    if x < xs[0].0 {
        return ys[0].0;
    }
    // NumPy starts the one-query search with guess zero, then clamps it to one.
    let j = if n <= 4 {
        let mut i = 1;
        while i < n && x >= xs[i].0 {
            i += 1;
        }
        i - 1
    } else if x < xs[1].0 {
        0
    } else if x < xs[2].0 {
        1
    } else if x < xs[3].0 {
        2
    } else {
        let mut low = 3;
        let mut high = if n > 10 && x < xs[9].0 { 9 } else { n };
        while low < high {
            let mid = (low + high) / 2;
            if x >= xs[mid].0 {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        low - 1
    };
    if j == n - 1 || x == xs[j].0 {
        return ys[j].0;
    }
    let slope = (ys[j + 1].0 - ys[j].0) / (xs[j + 1].0 - xs[j].0);
    let mut y = slope * (x - xs[j].0) + ys[j].0;
    if y.is_nan() {
        y = slope * (x - xs[j + 1].0) + ys[j + 1].0;
        if y.is_nan() && ys[j].0 == ys[j + 1].0 {
            y = ys[j].0
        }
    }
    y
}
