//! Temporal complete-link grouping and physical identity assignment.
use super::{
    ancestry::{pair_map, Ancestry},
    assignment, graph, group_cluster,
    group_config::Config,
    group_pairs::{self, Bundle, Evidence, Pair},
    physical::{self, Object, VisionCue},
    retention::{self, Retention},
    tracking::Track,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Serialize, Deserialize)]
pub struct State {
    pub observation: Object,
    pub member_last_seen: BTreeMap<u32, i64>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Manager {
    pub config: Config,
    pub next_id: u32,
    pub now_ns: Option<i64>,
    #[serde(with = "pair_map")]
    pub pairs: BTreeMap<Pair, Evidence>,
    pub states: BTreeMap<u32, State>,
    pub stats: BTreeMap<String, usize>,
    pub last_pair_possible: usize,
    pub last_pair_candidates: usize,
    pub last_conflicts: usize,
    pub last_direct_carries: usize,
    pub last_multi_count: usize,
    #[serde(with = "pair_map")]
    pub provisional_bundles: BTreeMap<Pair, Bundle>,
    pub mature_retention: Option<Retention>,
    pub provisional_hidden: BTreeMap<u32, u32>,
    #[serde(with = "pair_map")]
    pub provisional_pid_pairs: BTreeMap<Pair, Pair>,
    pub last_provisional_decisions: Vec<Value>,
    pub provisional_created: usize,
    pub provisional_coherent: usize,
    pub provisional_handoffs: usize,
    pub provisional_releases: usize,
    #[serde(flatten)]
    pub ancestry: Ancestry,
    pub trace_decisions: bool,
    pub last_decisions: Vec<Value>,
}
impl Manager {
    pub fn new(config: Config) -> Result<Self, String> {
        config.validate()?;
        Ok(Self {
            next_id: config.first_physical_id,
            mature_retention: config.mature_retention_enabled.then(Retention::default),
            config,
            now_ns: None,
            pairs: BTreeMap::new(),
            states: BTreeMap::new(),
            stats: BTreeMap::new(),
            last_pair_possible: 0,
            last_pair_candidates: 0,
            last_conflicts: 0,
            last_direct_carries: 0,
            last_multi_count: 0,
            provisional_bundles: BTreeMap::new(),
            provisional_hidden: BTreeMap::new(),
            provisional_pid_pairs: BTreeMap::new(),
            last_provisional_decisions: vec![],
            provisional_created: 0,
            provisional_coherent: 0,
            provisional_handoffs: 0,
            provisional_releases: 0,
            ancestry: Ancestry::default(),
            trace_decisions: false,
            last_decisions: vec![],
        })
    }
    fn count(&mut self, key: &str, n: usize) {
        *self.stats.entry(key.into()).or_default() += n;
    }
    pub fn update(
        &mut self,
        timestamp: i64,
        raw: &[Track],
        yaw: Option<f64>,
        ego: f64,
        oem: Option<usize>,
        vision: &[VisionCue],
    ) -> Result<Vec<Object>, &'static str> {
        if timestamp < 0 {
            return Err("timestamp must be nonnegative integer nanoseconds");
        }
        if self.now_ns.is_some_and(|now| timestamp <= now) {
            return Err("group updates require strictly increasing scan timestamps");
        }
        if yaw.is_some_and(|v| !v.is_finite()) {
            return Err("yaw_rate must be finite or None");
        }
        if ego.is_infinite() {
            return Err("v_ego must be finite or NaN unknown");
        }
        if raw.len() > 32 {
            return Err("at most 32 fresh RawTrack observations are required");
        }
        if raw
            .iter()
            .map(|r| r.detection.slot)
            .collect::<BTreeSet<_>>()
            .len()
            != raw.len()
        {
            return Err("duplicate raw slot");
        }
        let coast = (self.config.coast_s * 1e9).round_ties_even() as i64;
        if raw.iter().any(|r| {
            r.detection.timestamp_ns > timestamp
                || timestamp - r.detection.timestamp_ns > coast
                || self
                    .now_ns
                    .is_some_and(|now| r.detection.timestamp_ns <= now)
        }) {
            return Err("stale, repeated or future raw observation");
        }
        if raw
            .iter()
            .map(|r| r.raw_track_id)
            .collect::<BTreeSet<_>>()
            .len()
            != raw.len()
        {
            return Err("duplicate raw track ID");
        }
        if u64::from(self.next_id) + raw.len() as u64 > 1u64 << 31 {
            let mut staged = self.clone();
            let result = staged.update_inner(timestamp, raw, yaw, ego, oem, vision)?;
            *self = staged;
            Ok(result)
        } else {
            self.update_inner(timestamp, raw, yaw, ego, oem, vision)
        }
    }
    fn update_inner(
        &mut self,
        timestamp: i64,
        input: &[Track],
        yaw: Option<f64>,
        ego: f64,
        oem: Option<usize>,
        vision: &[VisionCue],
    ) -> Result<Vec<Object>, &'static str> {
        let mut raw = input.to_vec();
        raw.sort_by_key(|r| r.raw_track_id);
        self.now_ns = Some(timestamp);
        let coast = (self.config.coast_s * 1e9).round_ties_even() as i64;
        let before = self.states.len();
        self.states.retain(|_, s| {
            if timestamp - s.observation.timestamp_ns > coast {
                return false;
            }
            s.member_last_seen.retain(|_, ns| timestamp - *ns <= coast);
            true
        });
        let deleted = before - self.states.len();
        if deleted > 0 {
            self.count("deleted", deleted);
        }
        let n = raw.len();
        let ids: Vec<_> = raw.iter().map(|r| r.raw_track_id).collect();
        let index: BTreeMap<_, _> = ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        let mut pair = group_pairs::update(
            &group_pairs::Context {
                timestamp,
                yaw,
                ego,
                oem,
            },
            &raw,
            &self.config,
            &mut self.pairs,
            &self.provisional_bundles,
        );
        self.last_pair_possible = n * n.saturating_sub(1) / 2;
        self.last_pair_candidates = pair.pair_candidates;
        self.count(
            "pair_rejected_distance_diameter",
            self.last_pair_possible - pair.pair_candidates,
        );
        self.count("pair_rejected_lateral_diameter", pair.lateral_rejections);
        self.count("pair_rejected_velocity_diameter", pair.velocity_rejections);
        self.count(
            "pair_rejected_stationary_moving_conflict",
            pair.motion_rejections,
        );
        if let Some(retention) = &mut self.mature_retention {
            retention.context = retention::Context(yaw, oem, vision.to_vec());
            let previous: Vec<_> = self.states.values().map(|s| &s.observation).collect();
            retention.before(
                retention::Frame {
                    config: &self.config,
                    timestamp,
                    raw: &raw,
                    previous: &previous,
                    index: &index,
                    ego,
                },
                &mut pair.compatible,
                &mut pair.pair_cost,
            )?;
        }
        let owner: BTreeMap<_, _> = self
            .states
            .iter()
            .flat_map(|(pid, s)| s.member_last_seen.keys().map(move |rid| (*rid, *pid)))
            .collect();
        let previous: Vec<_> = self.states.keys().copied().collect();
        let clustered = group_cluster::merge(
            &pair.compatible,
            &pair.pair_cost,
            &ids,
            &owner,
            self.config.max_members,
        );
        let clusters = &clustered.clusters;
        let mut assigned = BTreeMap::new();
        let mut carry = None;
        let mut solver_scores = None;
        self.last_conflicts = 0;
        self.last_direct_carries = 0;
        if !previous.is_empty() && !clusters.is_empty() {
            let mut claims = BTreeMap::new();
            let mut ambiguous = BTreeSet::new();
            for (i, owners) in clustered.owners.iter().enumerate() {
                if owners.len() > 1 {
                    ambiguous.insert(i);
                }
                for pid in owners {
                    if let Some(prior) = claims.get(pid) {
                        ambiguous.insert(i);
                        ambiguous.insert(*prior);
                    } else {
                        claims.insert(*pid, i);
                    }
                }
            }
            self.last_conflicts = ambiguous.len();
            if ambiguous.is_empty() {
                for (i, owners) in clustered.owners.iter().enumerate() {
                    if let Some(pid) = owners.first() {
                        assigned.insert(i, *pid);
                    }
                }
                self.last_direct_carries = assigned.len();
                carry = Some("direct_carry");
            } else {
                carry = Some("assignment_solver");
                let mut holders: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
                for (j, pid) in previous.iter().enumerate() {
                    for rid in self.states[pid].member_last_seen.keys() {
                        holders.entry(*rid).or_default().push(j);
                    }
                }
                let mut row_edges = vec![];
                let mut column_edges = vec![vec![]; previous.len()];
                for (i, cluster) in clusters.iter().enumerate() {
                    let members: BTreeSet<_> = cluster.iter().map(|k| ids[*k]).collect();
                    let mut overlaps: BTreeMap<usize, usize> = BTreeMap::new();
                    for rid in &members {
                        for j in holders.get(rid).into_iter().flatten() {
                            *overlaps.entry(*j).or_default() += 1;
                        }
                    }
                    let mut edges = vec![];
                    for (j, overlap) in overlaps {
                        let pid = previous[j];
                        let obs = &self.states[&pid].observation;
                        let score = 10.0 * overlap as f64
                            + 3.0
                                * u8::from(members.contains(&obs.representative_raw_track_id))
                                    as f64
                            + obs.age_scans.min(1000) as f64 * 1e-5
                            + 1.0 / (f64::from(pid) + 1.0);
                        edges.push((j, score));
                        column_edges[j].push((i, score));
                    }
                    row_edges.push(edges);
                }
                let matched = if self.trace_decisions {
                    None
                } else {
                    graph::physical(&row_edges, &column_edges)
                };
                if let Some(matched) = matched {
                    for (i, j) in matched {
                        assigned.insert(i, previous[j]);
                    }
                } else {
                    let mut scores =
                        vec![vec![0.; previous.len() + clusters.len()]; clusters.len()];
                    for (i, edges) in row_edges.iter().enumerate() {
                        for &(j, s) in edges {
                            scores[i][j] = s;
                        }
                    }
                    let negative: Vec<Vec<_>> = scores
                        .iter()
                        .map(|row| row.iter().map(|v| -v).collect())
                        .collect();
                    let solved = assignment::solve(&negative, previous.len() + clusters.len())?;
                    for (i, j) in solved.rows.into_iter().zip(solved.columns) {
                        if j < previous.len() && scores[i][j] > 0. {
                            assigned.insert(i, previous[j]);
                        }
                    }
                    if self.trace_decisions {
                        solver_scores = Some(scores);
                    }
                }
            }
        }
        let vision_supported: Vec<_> = raw
            .iter()
            .map(|r| super::group_config::vision_support(r, vision))
            .collect();
        let current: BTreeSet<_> = ids.iter().copied().collect();
        let mut result = vec![];
        let mut decisions = vec![];
        let mut largest = *self.stats.get("max_group_size").unwrap_or(&0);
        for (i, cluster) in clusters.iter().enumerate() {
            let old = assigned
                .get(&i)
                .and_then(|pid| self.states.get(pid))
                .cloned();
            let prior = old.as_ref().map(|s| &s.observation);
            let pid = if let Some(pid) = assigned.get(&i) {
                *pid
            } else {
                if self.next_id >= 1 << 31 {
                    return Err("physical Int32 ID space exhausted; no reuse");
                }
                let pid = self.next_id;
                self.next_id += 1;
                self.count("created", 1);
                pid
            };
            let mut ordered = cluster.clone();
            if ordered.len() > 1 {
                ordered.sort_unstable();
            }
            let candidates: Vec<_> = ordered.iter().map(|k| raw[*k].clone()).collect();
            let member_ids: BTreeSet<_> = candidates.iter().map(|m| m.raw_track_id).collect();
            let predicted = prior
                .filter(|_| candidates.len() > 1 || self.trace_decisions)
                .map(|p| project(timestamp, p.timestamp_ns, p.d_rel, p.y_rel, p.v_rel, yaw));
            let rep = physical::representative(
                &candidates,
                &index,
                &vision_supported,
                oem,
                prior.zip(predicted.map(|(_, x, y)| (x, y))),
            )?;
            let supported = cluster.iter().any(|k| vision_supported[*k]);
            let obj = Object {
                physical_track_id: pid,
                timestamp_ns: timestamp,
                members: candidates.clone(),
                representative_raw_track_id: rep.raw_track_id,
                d_rel: rep.detection.d_rel,
                y_rel: rep.detection.y_rel,
                v_rel: rep.detection.v_rel,
                oem_selected: candidates.iter().any(|r| oem == Some(r.detection.slot)),
                vision_supported: supported,
                age_scans: prior.map_or(1, |p| p.age_scans + 1),
                grouping_evidence: if candidates.len() == 1 {
                    "single_return"
                } else {
                    "temporal_complete_link"
                }
                .into(),
                published_surface: None,
            };
            if self.trace_decisions {
                decisions.push(decision(
                    &obj,
                    old.as_ref(),
                    carry,
                    predicted,
                    solver_scores.as_ref().map(|s| &s[i][..previous.len()]),
                ));
            }
            let state = self.states.entry(pid).or_insert_with(|| State {
                observation: obj.clone(),
                member_last_seen: BTreeMap::new(),
            });
            state
                .member_last_seen
                .retain(|rid, _| !current.contains(rid) || member_ids.contains(rid));
            state.observation = obj.clone();
            for m in &candidates {
                state.member_last_seen.insert(m.raw_track_id, timestamp);
            }
            if prior.is_some_and(|p| p.representative_raw_track_id != rep.raw_track_id) {
                self.count("representative_changes", 1);
            }
            let recovered = if candidates.len() == 1 {
                old.is_some() && rep.recovered
            } else {
                let owners: BTreeSet<_> = candidates
                    .iter()
                    .filter_map(|m| owner.get(&m.raw_track_id))
                    .collect();
                if owners.len() > 1 {
                    self.count("membership_merge_events", owners.len() - 1);
                }
                self.count("multi_member_groups", 1);
                old.is_some() && candidates.iter().all(|r| r.recovered)
            };
            if recovered {
                self.count("coasting_recoveries", 1);
            }
            if supported {
                self.count("vision_supported_groups", 1);
            }
            if ego.is_finite() && (obj.v_rel + ego).abs() <= self.config.stationary_speed_mps {
                self.count("stationary_groups", 1);
            }
            largest = largest.max(candidates.len());
            result.push(obj);
        }
        if largest != *self.stats.get("max_group_size").unwrap_or(&0) {
            self.stats.insert("max_group_size".into(), largest);
        }
        self.update_bundles(BundleFrame {
            timestamp,
            raw: &raw,
            index: &index,
            vision: &vision_supported,
            pair: &pair,
            objects: &result,
            yaw,
            oem,
        })?;
        let live: BTreeMap<_, _> = result
            .iter()
            .flat_map(|o| {
                o.members
                    .iter()
                    .map(move |m| (m.raw_track_id, o.physical_track_id))
            })
            .collect();
        let outputs: BTreeSet<_> = result.iter().map(|o| o.physical_track_id).collect();
        let before = self.states.len();
        self.states.retain(|pid, s| {
            s.member_last_seen
                .retain(|rid, _| live.get(rid).is_none_or(|owner| owner == pid));
            !s.member_last_seen.is_empty() || outputs.contains(pid)
        });
        let absorbed = before - self.states.len();
        if absorbed > 0 {
            self.count("absorbed", absorbed);
        }
        let states = self
            .states
            .iter()
            .map(|(pid, s)| (*pid, s.member_last_seen.keys().copied().collect()))
            .collect();
        self.ancestry.update(timestamp, &states, &result);
        self.count("scans", 1);
        self.count("output_objects", result.len());
        self.last_multi_count = result.iter().filter(|o| o.members.len() > 1).count();
        self.last_decisions = decisions;
        result.sort_by_key(|o| o.physical_track_id);
        Ok(result)
    }
}
fn project(
    timestamp: i64,
    prior: i64,
    d: f64,
    y: f64,
    v: f64,
    yaw: Option<f64>,
) -> (f64, f64, f64) {
    let dt = (timestamp - prior) as f64 / 1e9;
    let angle = -yaw.unwrap_or(0.) * dt;
    let dx = d + v * dt;
    let (s, c) = angle.sin_cos();
    (dt, dx * c - y * s, dx * s + y * c)
}
fn decision(
    obj: &Object,
    old: Option<&State>,
    carry: Option<&str>,
    predicted: Option<(f64, f64, f64)>,
    scores: Option<&[f64]>,
) -> Value {
    let prior = old.map(|s| &s.observation);
    let members: BTreeSet<_> = obj.members.iter().map(|m| m.raw_track_id).collect();
    let mut ranked: Vec<_> = scores
        .into_iter()
        .flatten()
        .filter(|v| **v > 0.)
        .copied()
        .collect();
    ranked.sort_by(|a, b| b.total_cmp(a));
    let xy = predicted.map(|(_, x, y)| (x, y));
    json!({"timestamp_ns":obj.timestamp_ns,"physical_track_id":obj.physical_track_id,"previous_physical_track_id":prior.map(|p|p.physical_track_id),"assignment_mode":if prior.is_none(){Some("created")}else{carry},"dt_s":predicted.map(|p|p.0).unwrap_or_else(||prior.map_or(0.,|p|(obj.timestamp_ns-p.timestamp_ns) as f64/1e9)),"member_raw_track_ids":obj.members.iter().map(|m|m.raw_track_id).collect::<Vec<_>>(),"member_slots":obj.members.iter().map(|m|m.detection.slot).collect::<Vec<_>>(),"previous_member_raw_track_ids":prior.map(|p|p.members.iter().map(|m|m.raw_track_id).collect::<Vec<_>>()).unwrap_or_default(),"member_overlap":old.map_or(0,|s|members.iter().filter(|id|s.member_last_seen.contains_key(id)).count()),"observed_member_overlap":prior.map_or(0,|p|p.members.iter().filter(|m|members.contains(&m.raw_track_id)).count()),"representative_raw_track_id":obj.representative_raw_track_id,"previous_representative_raw_track_id":prior.map(|p|p.representative_raw_track_id),"representative_changed":prior.is_some_and(|p|p.representative_raw_track_id!=obj.representative_raw_track_id),"representative_still_a_member":prior.is_some_and(|p|members.contains(&p.representative_raw_track_id)),"d_rel":obj.d_rel,"y_rel":obj.y_rel,"v_rel":obj.v_rel,"predicted_d_rel":xy.map(|p|p.0),"predicted_y_rel":xy.map(|p|p.1),"predicted_v_rel":prior.map(|p|p.v_rel),"residual_d_m":xy.map(|p|obj.d_rel-p.0),"residual_y_m":xy.map(|p|obj.y_rel-p.1),"residual_v_mps":predicted.and_then(|_|prior.map(|p|obj.v_rel-p.v_rel)),"best_score":ranked.first(),"second_score":ranked.get(1),"age_scans":obj.age_scans,"grouping_evidence":obj.grouping_evidence})
}
struct BundleFrame<'a> {
    timestamp: i64,
    raw: &'a [Track],
    index: &'a BTreeMap<u32, usize>,
    vision: &'a [bool],
    pair: &'a group_pairs::Result,
    objects: &'a [Object],
    yaw: Option<f64>,
    oem: Option<usize>,
}
impl Manager {
    fn update_bundles(&mut self, f: BundleFrame<'_>) -> Result<(), &'static str> {
        let mut decisions = vec![];
        let mut next = BTreeMap::new();
        let mut hidden = BTreeMap::new();
        let mut pids = BTreeMap::new();
        let by_raw: BTreeMap<_, _> = f
            .objects
            .iter()
            .flat_map(|o| o.members.iter().map(move |m| (m.raw_track_id, o)))
            .collect();
        let mut degree: BTreeMap<u32, usize> = BTreeMap::new();
        for &(a, b) in f.pair.provisional_matches.keys() {
            *degree.entry(a).or_default() += 1;
            *degree.entry(b).or_default() += 1;
        }
        let ambiguous = |key: Pair| {
            degree.get(&key.0).is_some_and(|n| *n > 1) || degree.get(&key.1).is_some_and(|n| *n > 1)
        };
        let mut handled = BTreeSet::new();
        for (&key, prior) in &self.provisional_bundles {
            if f.pair.provisional_matches.contains_key(&key) && !ambiguous(key) {
                continue;
            }
            handled.insert(key);
            let handoff = f.pair.provisional_handoffs.contains(&key);
            let reason = if handoff {
                "normal_group_handoff"
            } else if ambiguous(key) {
                "ambiguous_candidate"
            } else if !f.index.contains_key(&key.0) || !f.index.contains_key(&key.1) {
                "member_missing"
            } else if (f.timestamp - prior.timestamp_ns) as f64 > self.config.pair_max_gap_s * 1e9 {
                "scan_gap"
            } else if [key.0, key.1]
                .into_iter()
                .zip([prior.last_ages.0, prior.last_ages.1])
                .any(|(rid, age)| f.raw[f.index[&rid]].age_scans <= age)
            {
                "age_rollback"
            } else if f.oem.is_some_and(|slot| {
                [key.0, key.1]
                    .iter()
                    .any(|rid| f.raw[f.index[rid]].detection.slot == slot)
            }) {
                "oem_member_support"
            } else {
                "criteria_diverged"
            };
            if handoff {
                self.provisional_handoffs += 1;
            } else {
                self.provisional_releases += 1;
            }
            let physical: BTreeSet<_> = [key.0, key.1]
                .iter()
                .filter_map(|rid| by_raw.get(rid).map(|o| o.physical_track_id))
                .collect();
            decisions.push(provisional(
                f.timestamp,
                if handoff { "HANDOFF" } else { "RELEASE" },
                reason,
                key,
                physical.into_iter().collect(),
                (
                    Some(prior.representative_raw_track_id),
                    by_raw
                        .get(&prior.representative_raw_track_id)
                        .map(|o| o.physical_track_id),
                ),
                None,
            ));
        }
        for (&key, &(i, j, metrics)) in &f.pair.provisional_matches {
            if handled.contains(&key) {
                continue;
            }
            if ambiguous(key) {
                decisions.push(provisional(
                    f.timestamp,
                    "REJECT",
                    "ambiguous_candidate",
                    key,
                    vec![],
                    (None, None),
                    Some(metrics),
                ));
                continue;
            }
            let first = by_raw.get(&key.0).copied();
            let second = by_raw.get(&key.1).copied();
            let physical: Vec<_> = [first, second]
                .into_iter()
                .flatten()
                .map(|o| o.physical_track_id)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            let (Some(first), Some(second)) = (first, second) else {
                decisions.push(provisional(
                    f.timestamp,
                    "HANDOFF",
                    "normal_group_handoff",
                    key,
                    physical,
                    (None, None),
                    Some(metrics),
                ));
                self.provisional_handoffs += 1;
                continue;
            };
            if first.physical_track_id == second.physical_track_id {
                decisions.push(provisional(
                    f.timestamp,
                    "HANDOFF",
                    "normal_group_handoff",
                    key,
                    physical,
                    (None, None),
                    Some(metrics),
                ));
                self.provisional_handoffs += 1;
                continue;
            }
            if first.members.len() != 1 || second.members.len() != 1 {
                decisions.push(provisional(
                    f.timestamp,
                    "REJECT",
                    "existing_group_continuity",
                    key,
                    physical,
                    (None, None),
                    Some(metrics),
                ));
                if self.provisional_bundles.contains_key(&key) {
                    self.provisional_releases += 1;
                }
                continue;
            }
            let candidates = [f.raw[i].clone(), f.raw[j].clone()];
            let prior = self.provisional_bundles.get(&key);
            let anchor = prior.map(|p| {
                let (_, x, y) = project(
                    f.timestamp,
                    p.timestamp_ns,
                    p.d_rel,
                    p.y_rel,
                    p.v_rel,
                    f.yaw,
                );
                physical::Anchor {
                    representative_raw_track_id: p.representative_raw_track_id,
                    v_rel: p.v_rel,
                    predicted_xy: (x, y),
                }
            });
            let rep = physical::representative_from_anchor(
                &candidates,
                f.index,
                f.vision,
                f.oem,
                anchor,
            )?;
            let rep_pid = by_raw[&rep.raw_track_id].physical_track_id;
            let other = if rep_pid == first.physical_track_id {
                second.physical_track_id
            } else {
                first.physical_track_id
            };
            next.insert(
                key,
                Bundle {
                    raw_track_ids: key,
                    timestamp_ns: f.timestamp,
                    last_ages: (f.raw[i].age_scans, f.raw[j].age_scans),
                    representative_raw_track_id: rep.raw_track_id,
                    d_rel: rep.detection.d_rel,
                    y_rel: rep.detection.y_rel,
                    v_rel: rep.detection.v_rel,
                },
            );
            hidden.insert(other, rep_pid);
            pids.insert(key, (rep_pid, other));
            let action = if prior.is_none() {
                self.provisional_created += 1;
                "CREATE"
            } else {
                self.provisional_coherent += 1;
                "COHERENT"
            };
            decisions.push(provisional(
                f.timestamp,
                action,
                "",
                key,
                physical,
                (Some(rep.raw_track_id), Some(rep_pid)),
                Some(metrics),
            ));
        }
        self.provisional_bundles = next;
        self.provisional_hidden = hidden;
        self.provisional_pid_pairs = pids;
        self.last_provisional_decisions = decisions;
        Ok(())
    }
    pub fn provisional_publication_view(
        &mut self,
        objects: &[Object],
        strict: &BTreeMap<u32, usize>,
        oem: &BTreeSet<u32>,
    ) -> Vec<Object> {
        if self.provisional_hidden.is_empty() || objects.is_empty() {
            return objects.to_vec();
        }
        let present: BTreeMap<_, _> = objects.iter().map(|o| (o.physical_track_id, o)).collect();
        let mut hidden = self.provisional_hidden.clone();
        let pairs: Vec<_> = self
            .provisional_pid_pairs
            .iter()
            .map(|(k, v)| (*k, *v))
            .collect();
        for (key, (rep, other)) in pairs {
            let independent = match (strict.get(&rep), strict.get(&other)) {
                (Some(a), Some(b)) => a != b,
                (Some(_), None) | (None, Some(_)) => true,
                (None, None) => false,
            };
            let supported = [rep, other]
                .iter()
                .any(|pid| oem.contains(pid) || present.get(pid).is_some_and(|o| o.oem_selected));
            let reason = if !present.contains_key(&rep) || !present.contains_key(&other) {
                "publication_member_missing"
            } else if independent {
                "strict_camera_identity"
            } else if supported {
                "oem_identity"
            } else {
                continue;
            };
            let state = self.provisional_bundles.remove(&key);
            self.provisional_pid_pairs.remove(&key);
            hidden.remove(&other);
            self.provisional_releases += 1;
            let mut pids = vec![rep, other];
            pids.sort_unstable();
            self.last_provisional_decisions.push(provisional(
                objects[0].timestamp_ns,
                "RELEASE",
                reason,
                key,
                pids,
                (state.map(|s| s.representative_raw_track_id), Some(rep)),
                None,
            ));
        }
        self.provisional_hidden = hidden;
        objects
            .iter()
            .filter(|o| !self.provisional_hidden.contains_key(&o.physical_track_id))
            .cloned()
            .collect()
    }
    pub fn reset_provisional(&mut self, timestamp: i64, reason: &str) {
        if self.provisional_bundles.is_empty() {
            self.provisional_hidden.clear();
            self.provisional_pid_pairs.clear();
            return;
        }
        self.last_provisional_decisions = self
            .provisional_bundles
            .iter()
            .map(|(&key, state)| {
                let pair = self.provisional_pid_pairs.get(&key);
                let mut pids: Vec<_> = pair.into_iter().flat_map(|&(a, b)| [a, b]).collect();
                pids.sort_unstable();
                provisional(
                    timestamp,
                    "RELEASE",
                    reason,
                    key,
                    pids,
                    (Some(state.representative_raw_track_id), pair.map(|p| p.0)),
                    None,
                )
            })
            .collect();
        self.provisional_releases += self.provisional_bundles.len();
        self.provisional_bundles.clear();
        self.provisional_hidden.clear();
        self.provisional_pid_pairs.clear();
    }
    pub fn diagnostics(&self) -> Vec<Value> {
        self.pairs.iter().filter_map(|(&(a,b),e)| {let last=e.samples.back()?;if Some(last.0)!=self.now_ns {return None;}
   let min_d=e.samples.iter().map(|s|s.1.abs()).reduce(f64::min).unwrap();let min_y=e.samples.iter().map(|s|s.2.abs()).reduce(f64::min).unwrap();let stable=last.1.abs()-min_d<=self.config.max_relative_distance_growth_m&&last.2.abs()-min_y<=self.config.max_relative_lateral_growth_m;
   let mature=e.samples.len()>=self.config.min_pair_observations&&(last.0-e.samples.front().unwrap().0) as f64/1e9+1e-6>=self.config.min_pair_span_s;
   Some(json!({"a":a,"b":b,"compatible":stable&&mature,"reason":if !stable {"relative_motion_drift"}else if mature {"geometry"}else{"insufficient_temporal_evidence"}}))
  }).collect()
    }
}
fn provisional(
    timestamp: i64,
    action: &str,
    reason: &str,
    key: Pair,
    pids: Vec<u32>,
    representative: (Option<u32>, Option<u32>),
    metrics: Option<[f64; 5]>,
) -> Value {
    json!({"timestamp_ns":timestamp,"action":action,"release_reason":reason,"raw_track_ids":key,"physical_track_ids":pids,"representative_raw_track_id":representative.0,"representative_physical_track_id":representative.1,"delta_d_m":metrics.map(|m|m[0]),"delta_y_m":metrics.map(|m|m[1]),"delta_bearing_deg":metrics.map(|m|m[2]),"delta_vrel_mps":metrics.map(|m|m[3]),"delta_world_speed_mps":metrics.map(|m|m[4])})
}
