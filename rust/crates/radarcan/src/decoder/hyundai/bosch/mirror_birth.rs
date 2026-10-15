//! Wall/specular birth proof and bounded publication-only mirror hold.
use super::{birth_b5::Number, physical::Object, tracking::Track};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
pub type HistoryScan = (i64, Vec<(Number, Number, u32)>);
pub type MirrorObject = (f64, f64, f64, usize, Vec<u32>, u32, bool, bool);
#[derive(Clone, Serialize, Deserialize)]
pub struct Decision {
    pub physical_track_id: u32,
    pub action: String,
    pub reason: String,
    pub parent_pid: Option<u32>,
    pub wall_y_m: Option<f64>,
    pub residual_m: Option<f64>,
    pub count: usize,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Hold {
    pub physical_track_id: u32,
    pub parent_pid: u32,
    pub wall_y_m: f64,
    pub residual_m: f64,
    pub d_rel_m: f64,
    pub y_rel_m: f64,
    pub v_rel_mps: f64,
    pub birth_ns: i64,
    pub held_scans: usize,
    pub parent_miss_scans: usize,
    pub diverge_scans: usize,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct MirrorBirth {
    pub mode: u8,
    pub hist: Vec<HistoryScan>,
    pub holds: IndexMap<u32, Hold>,
    pub seen_births: BTreeSet<u32>,
    pub last_ns: Option<i64>,
    pub last_decisions: Vec<Decision>,
    pub last_mirror_objects: IndexMap<u32, MirrorObject>,
    pub last_decisions_by_pid: IndexMap<u32, Decision>,
    pub last_events: Vec<Value>,
    pub would_suppress: BTreeSet<u32>,
    pub reset_count: usize,
    pub hold_count: usize,
    pub release_count: usize,
    pub publication_suppressed: usize,
    pub peak_history_points: usize,
    pub peak_hold_count: usize,
    #[serde(rename = "_pending_events", skip_serializing_if = "Option::is_none")]
    pub pending_events: Option<Vec<Value>>,
}
pub struct Frame<'a> {
    pub objects: &'a [Object],
    pub raw: &'a [Track],
    pub timestamp: i64,
    pub ego: f64,
    pub yaw: Option<f64>,
    pub camera: &'a std::collections::BTreeMap<u32, (i32, i64)>,
    pub word0: &'a BTreeSet<u32>,
    pub live: Option<&'a BTreeSet<u32>>,
}
impl MirrorBirth {
    pub fn new(mode: u8) -> Result<Self, &'static str> {
        if mode > 2 {
            return Err("invalid Bosch mirror-birth mode");
        }
        Ok(Self {
            mode,
            hist: vec![],
            holds: IndexMap::new(),
            seen_births: BTreeSet::new(),
            last_ns: None,
            last_decisions: vec![],
            last_mirror_objects: IndexMap::new(),
            last_decisions_by_pid: IndexMap::new(),
            last_events: vec![],
            would_suppress: BTreeSet::new(),
            reset_count: 0,
            hold_count: 0,
            release_count: 0,
            publication_suppressed: 0,
            peak_history_points: 0,
            peak_hold_count: 0,
            pending_events: None,
        })
    }
    fn reset_shadow(&mut self) {
        self.hist.clear();
        self.would_suppress.clear();
        self.reset_count += 1;
    }
    pub fn reset(&mut self, timestamp: Option<i64>, reason: &str) {
        self.pending_events = None;
        let release = timestamp.or(self.last_ns);
        self.last_events = self
            .holds
            .values()
            .map(|s| event(s, "RELEASE", reason, release))
            .collect();
        self.release_count += self.holds.len();
        self.holds.clear();
        self.seen_births.clear();
        self.reset_shadow();
        self.last_ns = None;
        self.last_decisions.clear();
        self.last_mirror_objects.clear();
        self.last_decisions_by_pid.clear();
    }
    fn wall(
        &self,
        side: f64,
        low: f64,
        high: f64,
        x_of: impl Fn(f64) -> f64,
        excluded: &BTreeSet<u32>,
    ) -> (Option<f64>, &'static str, usize, f64) {
        let mut pts: Vec<_> = self
            .hist
            .iter()
            .flat_map(|(_, pts)| pts.iter())
            .filter(|(_, y, id)| {
                !excluded.contains(id) && low < side * y.0 && side * y.0 < high
            })
            .map(|(x, y, _)| (x.0, y.0))
            .collect();
        if pts.is_empty() {
            return (None, "NO_WALL_SUPPORT", 0, 0.);
        }
        pts.sort_by(|a, b| cmp(side * a.1, side * b.1));
        let mut failure = ("NO_WALL_SUPPORT", 0, 0.);
        let mut tried = BTreeSet::new();
        for &(_, y0) in &pts {
            let center = side * y0 + 0.5;
            let rounded = round_one(center);
            let key = if rounded == 0. { 0 } else { rounded.to_bits() };
            if !tried.insert(key) {
                continue;
            }
            let band: Vec<_> = pts
                .iter()
                .filter(|(_, y)| (side * y - center).abs() <= 0.5 + 1e-9)
                .copied()
                .collect();
            if band.is_empty() {
                continue;
            }
            let mut ys: Vec<_> = band.iter().map(|(_, y)| *y).collect();
            ys.sort_by(|a, b| cmp(*a, *b));
            let wall = ys[ys.len() / 2];
            let xr = x_of(wall);
            if !xr.is_finite() {
                continue;
            }
            let window: Vec<_> = band
                .iter()
                .map(|(x, _)| *x)
                .filter(|x| (*x - xr).abs() <= 6.)
                .collect();
            let bins: BTreeSet<_> = window
                .iter()
                .map(|x| (((x - (xr - 6.)) / 2.) as i64).clamp(0, 5))
                .collect();
            let cover = bins.len() as f64 / 6.;
            if window.len() >= 6 && cover >= 0.5 {
                return (Some(wall), "OK", window.len(), cover);
            }
            if window.len() > failure.1 {
                failure = ("WALL_NOT_AT_SPECULAR_POINT", window.len(), cover);
            }
        }
        (None, failure.0, failure.1, failure.2)
    }
    fn mirror_decisions(
        &mut self,
        f: &Frame<'_>,
    ) -> (IndexMap<u32, MirrorObject>, IndexMap<u32, Decision>) {
        let mirror: IndexMap<_, _> = f
            .objects
            .iter()
            .map(|o| {
                (
                    o.physical_track_id,
                    (
                        o.d_rel,
                        o.y_rel,
                        o.v_rel,
                        o.age_scans,
                        o.members.iter().map(|m| m.raw_track_id).collect(),
                        o.representative_raw_track_id,
                        o.oem_selected,
                        o.vision_supported,
                    ),
                )
            })
            .collect();
        let fresh: BTreeSet<_> = f.raw.iter().map(|r| r.raw_track_id).collect();
        let stationary = if let Some(yaw) = f.yaw.filter(|v| v.is_finite()) {
            f.raw
                .iter()
                .filter(|r| {
                    (r.detection.v_rel + f.ego - yaw * r.detection.y_rel).abs() <= 0.5
                        && r.detection.y_rel.abs() >= 1.
                })
                .map(|r| {
                    (
                        Number(r.detection.d_rel),
                        Number(r.detection.y_rel),
                        r.raw_track_id,
                    )
                })
                .collect()
        } else {
            vec![]
        };
        self.hist.push((f.timestamp, stationary));
        self.peak_history_points = self
            .peak_history_points
            .max(self.hist.iter().map(|(_, p)| p.len()).sum());
        let context = f
            .yaw
            .is_some_and(|y| y.is_finite() && f.ego >= 3. && (y / f.ego.max(0.1)).abs() <= 0.01);
        let yaw = f.yaw.filter(|v| v.is_finite()).unwrap_or(0.);
        let world = |o: &MirrorObject| o.2 + f.ego - yaw * o.1;
        let is_fresh = |o: &MirrorObject| o.4.iter().any(|id| fresh.contains(id));
        let mut decisions = IndexMap::new();
        for (&pid, obj) in &mirror {
            if obj.3 != 1 || self.seen_births.contains(&pid) {
                continue;
            }
            let mut reason = None;
            let (mut parent, mut wall, mut residual) = (None, None, None);
            if obj.1.abs() < 2. || obj.0 > 80. || world(obj) < 2. {
                reason = Some("NOT_APPLICABLE");
            } else if !context {
                reason = Some("CONTEXT_FAIL_OPEN");
            } else if !is_fresh(obj) {
                reason = Some("X_NOT_FRESH");
            } else if identity(pid, obj, f) {
                reason = Some("INDEPENDENT_IDENTITY_FAIL_OPEN");
            } else {
                let side = if obj.1 > 0. { 1. } else { -1. };
                let candidates: Vec<_> = mirror
                    .iter()
                    .filter(|(other, c)| {
                        **other != pid
                            && (c.0 - obj.0).abs() <= 1.5
                            && (c.2 - obj.2).abs() <= 0.5
                            && side * c.1 < side * obj.1 - 2.
                    })
                    .collect();
                if candidates.is_empty() {
                    reason = Some("NO_PARENT");
                } else {
                    let mut passing = vec![];
                    let mut last_fail = "PARENT_NOT_DIRECT";
                    for (&candidate_pid, candidate) in candidates {
                        if candidate.3 < 10 || !is_fresh(candidate) || world(candidate) < 2. {
                            continue;
                        }
                        let excluded = obj.4.iter().chain(&candidate.4).copied().collect();
                        let (found, wall_reason, support, cover) = self.wall(
                            side,
                            side * candidate.1 + 1.,
                            side * obj.1 - 1.,
                            |wall| {
                                if obj.1 != 0. {
                                    obj.0 * wall / obj.1
                                } else {
                                    f64::NAN
                                }
                            },
                            &excluded,
                        );
                        let Some(found) = found else {
                            let _ = (support, cover);
                            last_fail = wall_reason;
                            continue;
                        };
                        let found_residual = obj.1 - (2. * found - candidate.1);
                        if found_residual.abs() > 2. {
                            last_fail = "MIRROR_RESIDUAL";
                            wall = Some(found);
                            residual = Some(found_residual);
                            parent = Some(candidate_pid);
                            continue;
                        }
                        passing.push((found_residual.abs(), candidate_pid, found, found_residual));
                    }
                    if passing.is_empty() {
                        reason = Some(last_fail);
                    } else {
                        passing.sort_by(|a, b| {
                            cmp(a.0, b.0)
                                .then_with(|| a.1.cmp(&b.1))
                                .then_with(|| cmp(a.2, b.2))
                                .then_with(|| cmp(a.3, b.3))
                        });
                        let chosen = passing[0];
                        parent = Some(chosen.1);
                        wall = Some(chosen.2);
                        residual = Some(chosen.3);
                    }
                }
            }
            if reason != Some("NOT_APPLICABLE") {
                decisions.insert(
                    pid,
                    Decision {
                        physical_track_id: pid,
                        action: if reason.is_none() {
                            "QUALIFY"
                        } else {
                            "NO_DECISION"
                        }
                        .into(),
                        reason: reason.unwrap_or("OK").into(),
                        parent_pid: parent,
                        wall_y_m: wall,
                        residual_m: residual,
                        count: usize::from(reason.is_none()),
                    },
                );
            }
        }
        (mirror, decisions)
    }
    pub fn update(&mut self, f: Frame<'_>) -> Result<BTreeSet<u32>, &'static str> {
        let mut events = vec![];
        self.pending_events = None;
        self.last_decisions.clear();
        self.would_suppress.clear();
        if self.mode == 0 {
            self.holds.clear();
            self.seen_births.clear();
            self.hist.clear();
            self.last_ns = Some(f.timestamp);
            self.last_events.clear();
            self.last_mirror_objects.clear();
            self.last_decisions_by_pid.clear();
            return Ok(self.would_suppress.clone());
        }
        if let Some(last) = self.last_ns {
            let delta = f.timestamp - last;
            if delta <= 0 || delta >= 300_000_000 {
                for state in self.holds.values() {
                    events.push(event(state, "RELEASE", "GAP_RESET", Some(f.timestamp)));
                    self.release_count += 1;
                }
                self.holds.clear();
                self.seen_births.clear();
                self.reset_shadow();
            } else {
                let dt = delta as f64 * 1e-9;
                let theta = f.yaw.filter(|v| *v != 0.).unwrap_or(0.) * dt;
                if theta.is_infinite() {
                    self.pending_events = Some(events);
                    return Err("math domain error");
                }
                let cosine = theta.cos();
                let sine = theta.sin();
                let neg = -sine;
                let travel = f.ego * dt;
                let start = self.hist.len().saturating_sub(19);
                self.hist = self.hist[start..]
                    .iter()
                    .map(|(ns, pts)| {
                        (
                            *ns,
                            pts.iter()
                                .map(|(x, y, id)| {
                                    let x1 = x.0 - travel;
                                    (
                                        Number(cosine * x1 + sine * y.0),
                                        Number(neg * x1 + cosine * y.0),
                                        *id,
                                    )
                                })
                                .collect(),
                        )
                    })
                    .collect();
            }
        }
        self.last_ns = Some(f.timestamp);
        if let Some(live) = f.live {
            self.seen_births.retain(|id| live.contains(id));
        }
        let (mirror, decisions) = self.mirror_decisions(&f);
        self.last_mirror_objects = mirror;
        self.last_decisions_by_pid = decisions;
        let mirror = &self.last_mirror_objects;
        let mut removed = vec![];
        for (&pid, state) in &mut self.holds {
            let Some(obj) = mirror.get(&pid) else {
                events.push(event(state, "RELEASE", "PID_DEAD", Some(f.timestamp)));
                self.release_count += 1;
                removed.push(pid);
                continue;
            };
            state.held_scans += 1;
            let parent = mirror.get(&state.parent_pid);
            let side = if state.wall_y_m > 0. { 1. } else { -1. };
            let mut reason = if identity(pid, obj, &f) {
                Some("INDEPENDENT_IDENTITY")
            } else if side * obj.1 < side * state.wall_y_m + 0.5 || obj.1.abs() < 4.5 {
                Some("MOVED_INSIDE")
            } else {
                None
            };
            if reason.is_none() {
                state.parent_miss_scans = if parent.is_none() {
                    state.parent_miss_scans + 1
                } else {
                    0
                };
                if state.parent_miss_scans >= 2 {
                    reason = Some("PARENT_LOST");
                } else if let Some(parent) = parent {
                    let diverged = (obj.2 - parent.2).abs() > 1.5 || (obj.0 - parent.0).abs() > 5.;
                    state.diverge_scans = if diverged { state.diverge_scans + 1 } else { 0 };
                    if state.diverge_scans >= 3 {
                        reason = Some("DIVERGED");
                    }
                }
            }
            if reason.is_none() && state.held_scans > 10 {
                reason = Some("EXPIRED");
            }
            if let Some(reason) = reason {
                events.push(event(state, "RELEASE", reason, Some(f.timestamp)));
                self.release_count += 1;
                removed.push(pid);
            }
        }
        for pid in removed {
            self.holds.shift_remove(&pid);
        }
        for (&pid, obj) in mirror {
            if obj.3 != 1 || !self.seen_births.insert(pid) {
                continue;
            }
            let Some(decision) = self
                .last_decisions_by_pid
                .get(&pid)
                .filter(|d| d.action == "QUALIFY" && obj.1.abs() >= 4.5)
            else {
                continue;
            };
            let state = Hold {
                physical_track_id: pid,
                parent_pid: decision.parent_pid.expect("qualified parent"),
                wall_y_m: decision.wall_y_m.expect("qualified wall"),
                residual_m: decision.residual_m.expect("qualified residual"),
                d_rel_m: obj.0,
                y_rel_m: obj.1,
                v_rel_mps: obj.2,
                birth_ns: f.timestamp,
                held_scans: 1,
                parent_miss_scans: 0,
                diverge_scans: 0,
            };
            events.push(event(
                &state,
                "HOLD",
                "MIRROR_BIRTH_QUALIFIED",
                Some(f.timestamp),
            ));
            self.holds.insert(pid, state);
            self.hold_count += 1;
        }
        self.would_suppress = self.holds.keys().copied().collect();
        self.last_decisions = self.last_decisions_by_pid.values().cloned().collect();
        self.last_events = events;
        self.peak_hold_count = self.peak_hold_count.max(self.holds.len());
        Ok(self.would_suppress.clone())
    }
    pub fn publication_view(&mut self, objects: &[Object]) -> Vec<Object> {
        self.publication_by(objects, |o| o.physical_track_id)
    }
    pub fn publication_by<T: Clone>(&mut self, objects: &[T], id: impl Fn(&T) -> u32) -> Vec<T> {
        if self.mode != 2 || objects.is_empty() || self.would_suppress.is_empty() {
            return objects.to_vec();
        }
        let suppressed: BTreeSet<_> = objects
            .iter()
            .filter(|o| self.would_suppress.contains(&id(o)))
            .map(&id)
            .collect();
        self.publication_suppressed += suppressed.len();
        objects
            .iter()
            .filter(|o| !suppressed.contains(&id(o)))
            .cloned()
            .collect()
    }
}
fn event(s: &Hold, action: &str, reason: &str, timestamp: Option<i64>) -> Value {
    json!({"action":action,"ns":timestamp,"pid":s.physical_track_id,"parent":s.parent_pid,"wall_y":s.wall_y_m,"resid":s.residual_m,"d":s.d_rel_m,"y":s.y_rel_m,"v":s.v_rel_mps,"reason":reason,"held_scans":s.held_scans})
}
fn identity(pid: u32, obj: &MirrorObject, f: &Frame<'_>) -> bool {
    obj.7
        || obj.6
        || f.word0.contains(&pid)
        || f.camera
            .get(&pid)
            .is_some_and(|&(state, episode)| state == 1 && episode >= 0)
}
fn cmp(a: f64, b: f64) -> std::cmp::Ordering {
    a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
}
/// Decimal rounding is distinct from multiplying, rounding, then dividing.
pub fn round_one(value: f64) -> f64 {
    if !value.is_finite() {
        return value;
    }
    format!("{value:.1}")
        .parse()
        .expect("formatted finite decimal")
}
