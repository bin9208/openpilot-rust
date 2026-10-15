//! Two-scan mature-family proof; suppression affects publication only.
use super::{physical::Object, retention::path_offset};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Serialize, Deserialize)]
pub struct State {
    pub newborn_pid: u32,
    pub anchor_pid: u32,
    pub newborn_raw_id: u32,
    pub anchor_raw_ids: Vec<u32>,
    pub anchor_representative_raw_id: u32,
    pub first_ns: i64,
    pub last_ns: i64,
    pub newborn_age: usize,
    pub anchor_age: usize,
    pub delta_d_m: f64,
    pub delta_y_m: f64,
    pub delta_bearing_deg: f64,
    pub delta_vrel_mps: f64,
    pub delta_world_speed_mps: f64,
    pub d_path_m: f64,
    pub inward_scans: usize,
    pub inward_from_abs_path_m: f64,
    pub inward_since_ns: i64,
    pub active: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Family {
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
    pub handoffs: usize,
    pub publication_suppressed: usize,
    pub state_peak: usize,
}
pub struct Frame<'a> {
    pub objects: &'a [Object],
    pub timestamp: i64,
    pub ego: f64,
    pub yaw: Option<f64>,
    pub strict: &'a BTreeMap<u32, usize>,
    pub oem: &'a BTreeSet<u32>,
    pub excluded: &'a BTreeSet<u32>,
    pub path: &'a [(f64, f64)],
}
struct Decision<'a> {
    timestamp: i64,
    action: &'a str,
    reason: &'a str,
    state: &'a State,
    newborn: Option<&'a Object>,
    anchor: Option<&'a Object>,
    metrics: Option<[f64; 5]>,
    strict: &'a BTreeMap<u32, usize>,
    oem: &'a BTreeSet<u32>,
    path: Option<f64>,
    suppressed: bool,
}
impl Family {
    pub fn new(mode: u8) -> Result<Self, &'static str> {
        if mode > 2 {
            return Err("invalid Bosch family companion mode");
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
            handoffs: 0,
            publication_suppressed: 0,
            state_peak: 0,
        })
    }
    pub fn reset(&mut self, timestamp: i64, reason: &str) {
        let strict = BTreeMap::new();
        let oem = BTreeSet::new();
        let mut decisions = vec![];
        for state in self.states.values() {
            if state.active {
                decisions.push(decision(Decision {
                    timestamp,
                    action: "RELEASE",
                    reason,
                    state,
                    newborn: None,
                    anchor: None,
                    metrics: None,
                    strict: &strict,
                    oem: &oem,
                    path: None,
                    suppressed: false,
                }));
                self.releases += 1;
            }
        }
        self.states.clear();
        self.would_suppress.clear();
        self.last_decisions = decisions;
        self.last_ns = Some(timestamp);
    }
    pub fn update(&mut self, f: Frame<'_>) -> BTreeSet<u32> {
        self.would_suppress.clear();
        self.last_decisions.clear();
        self.pair_evaluations_last = 0;
        if self.mode == 0 {
            self.states.clear();
            self.last_ns = Some(f.timestamp);
            return self.would_suppress.clone();
        }
        if let Some(last) = self.last_ns {
            if f.timestamp <= last || f.timestamp - last > 160_000_000 {
                self.reset(
                    f.timestamp,
                    if f.timestamp <= last {
                        "STATE_RESET"
                    } else {
                        "SCAN_GAP"
                    },
                );
                return self.would_suppress.clone();
            }
        }
        self.last_ns = Some(f.timestamp);
        let mut oem = f.oem.clone();
        oem.extend(
            f.objects
                .iter()
                .filter(|o| o.oem_selected)
                .map(|o| o.physical_track_id),
        );
        let yaw = f.yaw.filter(|v| v.is_finite()).unwrap_or(0.);
        if !f.ego.is_finite() {
            self.reset(f.timestamp, "SPEED_DIVERGENCE");
            return self.would_suppress.clone();
        }
        let present: BTreeMap<_, _> = f.objects.iter().map(|o| (o.physical_track_id, o)).collect();
        let mut next = IndexMap::new();
        let mut suppress = BTreeSet::new();
        let mut decisions = self.last_decisions.clone();
        let mut handled = BTreeSet::new();
        for (&pid, prior) in &self.states {
            handled.insert(pid);
            let newborn = present.get(&pid).copied();
            let anchor = present.get(&prior.anchor_pid).copied();
            let early = if f.excluded.contains(&pid) {
                Some("CONTINUITY_LOSS")
            } else if newborn.is_none() {
                Some(
                    if anchor.is_some_and(|a| {
                        a.members
                            .iter()
                            .any(|m| m.raw_track_id == prior.newborn_raw_id)
                    }) {
                        "NORMAL_HANDOFF"
                    } else if anchor.is_none() {
                        "ANCHOR_LOST"
                    } else {
                        "CONTINUITY_LOSS"
                    },
                )
            } else if let (Some(newborn), Some(anchor)) = (newborn, anchor) {
                if newborn.age_scans <= prior.newborn_age
                    || anchor.age_scans <= prior.anchor_age
                    || (!prior.active && (prior.newborn_age != 1 || newborn.age_scans != 2))
                {
                    Some("STATE_RESET")
                } else if newborn.members.len() != 1
                    || newborn.members[0].raw_track_id != prior.newborn_raw_id
                    || anchor
                        .members
                        .iter()
                        .map(|m| m.raw_track_id)
                        .collect::<Vec<_>>()
                        != prior.anchor_raw_ids
                    || anchor.representative_raw_track_id != prior.anchor_representative_raw_id
                {
                    Some("CONTINUITY_LOSS")
                } else if strict_independent(pid, prior.anchor_pid, f.strict) {
                    Some("STRICT_CAMERA")
                } else if oem.contains(&pid) {
                    Some("OEM_IDENTITY")
                } else {
                    None
                }
            } else {
                Some("ANCHOR_LOST")
            };
            let reason = if let Some(reason) = early {
                reason
            } else {
                let newborn = newborn.unwrap();
                let anchor = anchor.unwrap();
                let metrics = metrics(newborn, anchor, f.ego, yaw);
                let world = world(newborn, f.ego, yaw);
                if metrics[3] > 0.75 || metrics[4] > 0.35 || world.abs() < 4. {
                    "SPEED_DIVERGENCE"
                } else if !geometry(metrics, world)
                    || (metrics[0] - prior.delta_d_m).abs() > 1.5
                    || (metrics[1] - prior.delta_y_m).abs() > 0.75
                {
                    "GEOMETRY_DIVERGENCE"
                } else {
                    let offset = path_offset(newborn.d_rel, newborn.y_rel, f.path);
                    let abs_path = offset.abs();
                    let (inward_scans, inward_from, inward_since) =
                        if abs_path < prior.d_path_m.abs() {
                            (
                                prior.inward_scans + 1,
                                if prior.inward_scans > 0 {
                                    prior.inward_from_abs_path_m
                                } else {
                                    prior.d_path_m.abs()
                                },
                                if prior.inward_scans > 0 {
                                    prior.inward_since_ns
                                } else {
                                    prior.last_ns
                                },
                            )
                        } else {
                            (0, abs_path, f.timestamp)
                        };
                    let rate = if inward_scans >= 3 {
                        (inward_from - abs_path)
                            / ((f.timestamp - inward_since) as f64 * 1e-9).max(1e-3)
                    } else {
                        0.
                    };
                    if rate >= 0.90 {
                        "INWARD_MOTION"
                    } else {
                        let active = prior.active || newborn.age_scans == 2;
                        let state = State {
                            newborn_pid: pid,
                            anchor_pid: prior.anchor_pid,
                            newborn_raw_id: prior.newborn_raw_id,
                            anchor_raw_ids: prior.anchor_raw_ids.clone(),
                            anchor_representative_raw_id: prior.anchor_representative_raw_id,
                            first_ns: prior.first_ns,
                            last_ns: f.timestamp,
                            newborn_age: newborn.age_scans,
                            anchor_age: anchor.age_scans,
                            delta_d_m: metrics[0],
                            delta_y_m: metrics[1],
                            delta_bearing_deg: metrics[2],
                            delta_vrel_mps: metrics[3],
                            delta_world_speed_mps: metrics[4],
                            d_path_m: offset,
                            inward_scans,
                            inward_from_abs_path_m: inward_from,
                            inward_since_ns: inward_since,
                            active,
                        };
                        next.insert(pid, state);
                        if active {
                            suppress.insert(pid);
                        }
                        if active && !prior.active {
                            self.holds += 1;
                            decisions.push(decision(Decision {
                                timestamp: f.timestamp,
                                action: "HOLD",
                                reason: "",
                                state: prior,
                                newborn: Some(newborn),
                                anchor: Some(anchor),
                                metrics: Some(metrics),
                                strict: f.strict,
                                oem: &oem,
                                path: Some(offset),
                                suppressed: true,
                            }));
                        }
                        continue;
                    }
                }
            };
            if prior.active {
                let action = if reason == "NORMAL_HANDOFF" {
                    self.handoffs += 1;
                    "HANDOFF"
                } else {
                    self.releases += 1;
                    "RELEASE"
                };
                decisions.push(decision(Decision {
                    timestamp: f.timestamp,
                    action,
                    reason,
                    state: prior,
                    newborn,
                    anchor,
                    metrics: None,
                    strict: f.strict,
                    oem: &oem,
                    path: newborn.map(|o| path_offset(o.d_rel, o.y_rel, f.path)),
                    suppressed: false,
                }));
            }
        }
        let mut anchors: Vec<_> = f
            .objects
            .iter()
            .filter(|o| o.age_scans >= 25 && !f.excluded.contains(&o.physical_track_id))
            .collect();
        anchors.sort_by(|a, b| {
            a.d_rel
                .partial_cmp(&b.d_rel)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for newborn in f.objects {
            let pid = newborn.physical_track_id;
            if handled.contains(&pid)
                || f.excluded.contains(&pid)
                || newborn.age_scans != 1
                || newborn.members.len() != 1
                || oem.contains(&pid)
            {
                continue;
            }
            let mut plausible = vec![];
            for &anchor in &anchors {
                if anchor.physical_track_id == pid || anchor.d_rel < newborn.d_rel - 12. {
                    continue;
                }
                if anchor.d_rel > newborn.d_rel + 12. {
                    break;
                }
                self.pair_evaluations_last += 1;
                let metrics = metrics(newborn, anchor, f.ego, yaw);
                if !strict_independent(pid, anchor.physical_track_id, f.strict)
                    && geometry(metrics, world(newborn, f.ego, yaw))
                {
                    plausible.push((anchor, metrics));
                }
            }
            let Some((anchor, metrics)) = plausible.into_iter().min_by(|(a, x), (b, y)| {
                b.age_scans
                    .cmp(&a.age_scans)
                    .then_with(|| x[0].partial_cmp(&y[0]).unwrap_or(std::cmp::Ordering::Equal))
                    .then_with(|| x[1].partial_cmp(&y[1]).unwrap_or(std::cmp::Ordering::Equal))
                    .then_with(|| a.physical_track_id.cmp(&b.physical_track_id))
            }) else {
                continue;
            };
            let offset = path_offset(newborn.d_rel, newborn.y_rel, f.path);
            next.insert(
                pid,
                State {
                    newborn_pid: pid,
                    anchor_pid: anchor.physical_track_id,
                    newborn_raw_id: newborn.members[0].raw_track_id,
                    anchor_raw_ids: anchor.members.iter().map(|m| m.raw_track_id).collect(),
                    anchor_representative_raw_id: anchor.representative_raw_track_id,
                    first_ns: f.timestamp,
                    last_ns: f.timestamp,
                    newborn_age: newborn.age_scans,
                    anchor_age: anchor.age_scans,
                    delta_d_m: metrics[0],
                    delta_y_m: metrics[1],
                    delta_bearing_deg: metrics[2],
                    delta_vrel_mps: metrics[3],
                    delta_world_speed_mps: metrics[4],
                    d_path_m: offset,
                    inward_scans: 0,
                    inward_from_abs_path_m: offset.abs(),
                    inward_since_ns: f.timestamp,
                    active: false,
                },
            );
        }
        self.states = next;
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
        let present: BTreeSet<_> = objects.iter().map(|o| o.physical_track_id).collect();
        let suppressed: BTreeSet<_> = self
            .would_suppress
            .iter()
            .filter(|pid| {
                present.contains(pid)
                    && self
                        .states
                        .get(*pid)
                        .is_some_and(|s| present.contains(&s.anchor_pid))
            })
            .copied()
            .collect();
        self.publication_suppressed += suppressed.len();
        objects
            .iter()
            .filter(|o| !suppressed.contains(&o.physical_track_id))
            .cloned()
            .collect()
    }
}
fn strict_independent(newborn: u32, anchor: u32, strict: &BTreeMap<u32, usize>) -> bool {
    strict
        .get(&newborn)
        .is_some_and(|value| strict.get(&anchor) != Some(value))
}
fn bearing(obj: &Object) -> f64 {
    obj.y_rel.atan2(obj.d_rel.max(0.5)).to_degrees()
}
fn world(obj: &Object, ego: f64, yaw: f64) -> f64 {
    obj.v_rel + ego - yaw * obj.y_rel
}
fn metrics(a: &Object, b: &Object, ego: f64, yaw: f64) -> [f64; 5] {
    [
        (a.d_rel - b.d_rel).abs(),
        (a.y_rel - b.y_rel).abs(),
        (bearing(a) - bearing(b)).abs(),
        (a.v_rel - b.v_rel).abs(),
        (world(a, ego, yaw) - world(b, ego, yaw)).abs(),
    ]
}
fn geometry(metrics: [f64; 5], speed: f64) -> bool {
    metrics[0] > 3.
        && metrics[0] <= 12.
        && metrics[1] <= 2.5
        && metrics[3] <= 0.75
        && metrics[4] <= 0.35
        && speed.abs() >= 4.
}
fn decision(d: Decision<'_>) -> Value {
    let s = d.state;
    json!({"timestamp_ns":d.timestamp,"action":d.action,"release_reason":d.reason,"newborn_pid":s.newborn_pid,"newborn_raw_id":s.newborn_raw_id,"anchor_pid":s.anchor_pid,"anchor_raw_ids":s.anchor_raw_ids,"delta_d_scan1_m":s.delta_d_m,"delta_d_scan2_m":d.metrics.map(|m|m[0]),"delta_y_scan1_m":s.delta_y_m,"delta_y_scan2_m":d.metrics.map(|m|m[1]),"delta_vrel_mps":d.metrics.map(|m|m[3]),"delta_world_speed_mps":d.metrics.map(|m|m[4]),"delta_bearing_deg":d.metrics.map(|m|m[2]),"newborn_age":d.newborn.map(|o|o.age_scans),"anchor_age":d.anchor.map(|o|o.age_scans),"anchor_member_count":d.anchor.map(|o|o.members.len()),"camera_coarse":d.newborn.is_some_and(|o|o.vision_supported),"camera_strict":strict_independent(s.newborn_pid,s.anchor_pid,d.strict),"oem_identity":d.oem.contains(&s.newborn_pid),"d_path_m":d.path,"public_suppressed":d.suppressed})
}
