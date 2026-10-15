//! Pinned camera A0/P2/G0/E2 state and independent publication eligibility.
use super::{
    birth_b5::Number,
    camera::CameraObject,
    camera_association::{associate, long_window, range_span, Measurement},
    physical::Object as PhysicalObject,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub type Association = (i32, i64, i64);
pub type Geometry = BTreeMap<Vec<u32>, (f64, f64, f64)>;
pub type Associations = BTreeMap<u32, Association>;
pub type Cameras = BTreeMap<i64, CameraObject>;
pub type Groups = Vec<BTreeSet<u32>>;
#[derive(Clone, Serialize, Deserialize)]
pub struct Object {
    pub physical_track_id: u32,
    pub timestamp_ns: i64,
    #[serde(
        serialize_with = "crate::scalar::serialize_float",
        deserialize_with = "crate::scalar::deserialize_float"
    )]
    pub d_rel: f64,
    #[serde(
        serialize_with = "crate::scalar::serialize_float",
        deserialize_with = "crate::scalar::deserialize_float"
    )]
    pub y_rel: f64,
    #[serde(
        serialize_with = "crate::scalar::serialize_float",
        deserialize_with = "crate::scalar::deserialize_float"
    )]
    pub v_rel: f64,
    pub age_scans: usize,
    pub oem_selected: bool,
    pub vision_supported: bool,
}
impl From<&PhysicalObject> for Object {
    fn from(o: &PhysicalObject) -> Self {
        Self {
            physical_track_id: o.physical_track_id,
            timestamp_ns: o.timestamp_ns,
            d_rel: o.d_rel,
            y_rel: o.y_rel,
            v_rel: o.v_rel,
            age_scans: o.age_scans,
            oem_selected: o.oem_selected,
            vision_supported: o.vision_supported,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct History {
    pub members: Vec<u32>,
    pub cam_key: i64,
    pub class_code: i64,
    pub last_confirm_ns: i64,
    pub stable_intervals: usize,
    pub sample_ns: i64,
    pub observations: Vec<(Number, Number, Number)>,
    pub oem_anchor: i64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct CurveHistory {
    pub camera_id: usize,
    pub episode: i64,
    pub last_assigned_ns: i64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct TruckHistory {
    pub members: Vec<u32>,
    pub camera_id: usize,
    pub episode: i64,
    pub last_ns: i64,
    pub confirmations: usize,
    pub dd: Number,
    pub dy: Number,
    pub dv: Number,
    pub camera_d: Number,
    pub camera_y: Number,
    pub camera_v: Number,
    pub camera_width: Number,
    pub recovery_age: usize,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Representative {
    pub members: BTreeSet<u32>,
    pub representative_pid: u32,
    pub timestamp_ns: i64,
    pub d_rel: Number,
    pub y_rel: Number,
    pub v_rel: Number,
    pub age: usize,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Extended {
    pub mode: u8,
    pub curve_reacquire_mode: u8,
    #[serde(with = "tuple_keys")]
    pub histories: BTreeMap<Vec<u32>, History>,
    #[serde(with = "tuple_keys")]
    pub truck_pair_histories: BTreeMap<Vec<u32>, TruckHistory>,
    pub representatives: Vec<Representative>,
    pub last_ns: Option<i64>,
    pub last_groups: Vec<Vec<u32>>,
    pub last_association_count: usize,
    pub last_associations: Associations,
    pub last_footprint_associations: Vec<u32>,
    pub last_candidate_count: usize,
    pub last_coast_count: usize,
    pub max_state_count: usize,
    pub representative_switches: usize,
    pub max_representative_jump: [Number; 3],
    pub mature_groups: Vec<Vec<u32>>,
    pub maturity_resets: usize,
    pub last_maturity_resets: usize,
    pub last_camera_ns: Option<i64>,
    pub last_truck_edge_count: usize,
    pub max_truck_pair_state: usize,
    pub last_truck_recovery_count: usize,
    pub last_camera_by_episode: Cameras,
    pub last_v_ego: Number,
    pub last_yaw_rate: Option<Number>,
    pub curve_reacquire_histories: BTreeMap<u32, CurveHistory>,
    pub curve_reacquire_state_peak: usize,
    pub curve_reacquire_attempts: usize,
    pub curve_reacquire_successes: usize,
    pub curve_reacquire_rejects: BTreeMap<String, usize>,
    pub last_baseline_associations: Associations,
    pub last_curve_reacquire: Vec<(u32, usize, i64)>,
    pub last_curve_reacquire_would: Vec<(u32, usize, i64)>,
}
#[derive(Clone, Copy)]
pub struct Snapshot<'a> {
    pub objects: &'a [CameraObject],
    pub count: usize,
    pub complete_ns: i64,
}
mod tuple_keys {
    use super::*;
    pub fn serialize<S: serde::Serializer, T: Serialize>(
        v: &BTreeMap<Vec<u32>, T>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        s.collect_map(v.iter().map(|(key, value)| {
            let mut body = key
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            if key.len() == 1 {
                body.push(',');
            }
            (format!("({body})"), value)
        }))
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
        d: D,
    ) -> Result<BTreeMap<Vec<u32>, T>, D::Error> {
        let rows = BTreeMap::<String, T>::deserialize(d)?;
        rows.into_iter()
            .map(|(key, value)| {
                let body = key
                    .strip_prefix('(')
                    .and_then(|v| v.strip_suffix(')'))
                    .ok_or_else(|| <D::Error as serde::de::Error>::custom("expected tuple key"))?;
                let ids = body
                    .split(',')
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(|v| v.parse().map_err(<D::Error as serde::de::Error>::custom))
                    .collect::<Result<Vec<u32>, D::Error>>()?;
                Ok((ids, value))
            })
            .collect()
    }
}
fn cmp(a: f64, b: f64) -> std::cmp::Ordering {
    a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
}
fn pmin(a: f64, b: f64) -> f64 {
    if b < a {
        b
    } else {
        a
    }
}
fn pmax(a: f64, b: f64) -> f64 {
    if b > a {
        b
    } else {
        a
    }
}
fn pair(a: u32, b: u32) -> Vec<u32> {
    vec![a.min(b), a.max(b)]
}
fn pairs(members: &[u32]) -> Vec<Vec<u32>> {
    let mut out = vec![];
    for (i, &a) in members.iter().enumerate() {
        for &b in &members[i + 1..] {
            out.push(pair(a, b));
        }
    }
    out
}
fn trig(angle: f64) -> Result<(f64, f64), &'static str> {
    if angle.is_infinite() {
        Err("math domain error")
    } else {
        Ok((angle.cos(), angle.sin()))
    }
}
impl Extended {
    pub fn new(mode: u8, curve: u8) -> Result<Self, &'static str> {
        if mode > 3 {
            return Err("invalid Bosch camera extended-grouping mode");
        }
        if curve > 2 {
            return Err("invalid Bosch camera curve-reacquire mode");
        }
        Ok(Self {
            mode,
            curve_reacquire_mode: curve,
            histories: BTreeMap::new(),
            truck_pair_histories: BTreeMap::new(),
            representatives: vec![],
            last_ns: None,
            last_groups: vec![],
            last_association_count: 0,
            last_associations: BTreeMap::new(),
            last_footprint_associations: vec![],
            last_candidate_count: 0,
            last_coast_count: 0,
            max_state_count: 0,
            representative_switches: 0,
            max_representative_jump: [Number(0.); 3],
            mature_groups: vec![],
            maturity_resets: 0,
            last_maturity_resets: 0,
            last_camera_ns: None,
            last_truck_edge_count: 0,
            max_truck_pair_state: 0,
            last_truck_recovery_count: 0,
            last_camera_by_episode: BTreeMap::new(),
            last_v_ego: Number(f64::NAN),
            last_yaw_rate: None,
            curve_reacquire_histories: BTreeMap::new(),
            curve_reacquire_state_peak: 0,
            curve_reacquire_attempts: 0,
            curve_reacquire_successes: 0,
            curve_reacquire_rejects: BTreeMap::new(),
            last_baseline_associations: BTreeMap::new(),
            last_curve_reacquire: vec![],
            last_curve_reacquire_would: vec![],
        })
    }
    fn reject_curve(&mut self, reason: &str) -> Association {
        *self
            .curve_reacquire_rejects
            .entry(reason.into())
            .or_default() += 1;
        (0, -1, -1)
    }
    pub fn geometry(a: &Object, b: &Object, ego: f64, yaw: Option<f64>) -> (f64, f64, f64, bool) {
        let dd = (a.d_rel - b.d_rel).abs();
        let dy = (a.y_rel - b.y_rel).abs();
        let dv = (a.v_rel - b.v_rel).abs();
        let mut conflict = false;
        if ego.is_finite() {
            let yaw = yaw.filter(|v| v.is_finite()).unwrap_or(0.);
            let wa = (a.v_rel + ego - yaw * a.y_rel).abs();
            let wb = (b.v_rel + ego - yaw * b.y_rel).abs();
            conflict = pmin(wa, wb) <= 0.6 && pmax(wa, wb) >= 1.4;
        }
        (
            dd,
            dy,
            dv,
            3. < dd && dd <= 12. && dy <= 1.5 && dv <= 1.5 && !conflict,
        )
    }
    fn curve_reacquire(
        &mut self,
        obj: &Object,
        cameras: &[CameraObject],
        count: usize,
        timestamp: i64,
        yaw: Option<f64>,
    ) -> Result<Association, &'static str> {
        self.curve_reacquire_attempts += 1;
        let Some(history) = self.curve_reacquire_histories.get(&obj.physical_track_id) else {
            return Ok(self.reject_curve("history"));
        };
        if !(0..=800_000_000).contains(&(timestamp - history.last_assigned_ns)) {
            return Ok(self.reject_curve("history"));
        }
        if !obj.oem_selected {
            return Ok(self.reject_curve("word1"));
        }
        if !(70. <= obj.d_rel
            && obj.d_rel < 100.
            && yaw.is_some_and(|v| v.is_finite() && v.abs() >= 0.015))
        {
            return Ok(self.reject_curve("context"));
        }
        let bearing = (-obj.y_rel).atan2(pmax(obj.d_rel, 0.5));
        let span = range_span(obj.d_rel);
        let mut matches = vec![];
        for i in 0..count {
            let c = cameras.get(i).ok_or("list index out of range")?;
            if c.obj_id != history.camera_id || c.episode != history.episode {
                continue;
            }
            let (pos, neg) = long_window(span, c.width_m);
            let long = c.long_m - obj.d_rel;
            let lat = c.lat_m + obj.y_rel;
            let lo = pmin(c.angle_left, c.angle_right);
            let hi = pmax(c.angle_left, c.angle_right);
            let outside = pmax(lo - bearing, bearing - hi);
            if -neg <= long && long <= pos && lat.abs() <= 2.35 && outside <= 0.026 {
                matches.push(c)
            }
        }
        if matches.len() != 1 {
            return Ok(self.reject_curve(if matches.len() > 1 {
                "ambiguous"
            } else {
                "geometry"
            }));
        }
        let c = matches[0];
        Ok((1, c.episode, c.class_code))
    }
    fn curve_prepare(
        &mut self,
        timestamp: i64,
        by_pid: &BTreeMap<u32, &Object>,
        cameras: &[CameraObject],
        count: usize,
    ) -> Result<(), &'static str> {
        if self.curve_reacquire_mode == 0 {
            self.curve_reacquire_histories.clear();
            return Ok(());
        }
        let mut keys = BTreeSet::new();
        for i in 0..count {
            let c = cameras.get(i).ok_or("list index out of range")?;
            keys.insert((c.obj_id, c.episode));
        }
        self.curve_reacquire_histories.retain(|pid, h| {
            by_pid.contains_key(pid)
                && keys.contains(&(h.camera_id, h.episode))
                && (0..=800_000_000).contains(&(timestamp - h.last_assigned_ns))
        });
        Ok(())
    }
    fn curve_update(&mut self, timestamp: i64, baseline: &Associations, cameras: &Cameras) {
        if self.curve_reacquire_mode == 0 {
            return;
        }
        for (&pid, verdict) in baseline {
            if verdict.0 == 1 {
                if let Some(c) = cameras.get(&verdict.1) {
                    self.curve_reacquire_histories.insert(
                        pid,
                        CurveHistory {
                            camera_id: c.obj_id,
                            episode: c.episode,
                            last_assigned_ns: timestamp,
                        },
                    );
                }
            } else if verdict.0 == 2 {
                self.curve_reacquire_histories.remove(&pid);
            }
        }
        if self.curve_reacquire_histories.len() > 64 {
            let mut rows: Vec<_> = self
                .curve_reacquire_histories
                .iter()
                .map(|(&pid, h)| (pid, h.clone()))
                .collect();
            rows.sort_by_key(|(pid, h)| (std::cmp::Reverse(h.last_assigned_ns), *pid));
            rows.truncate(64);
            self.curve_reacquire_histories = rows.into_iter().collect();
        }
        self.curve_reacquire_state_peak = self
            .curve_reacquire_state_peak
            .max(self.curve_reacquire_histories.len());
    }
    pub fn complete_link(
        objects: &[Object],
        edges: &BTreeMap<Vec<u32>, f64>,
        previous: &[BTreeSet<u32>],
    ) -> Groups {
        let mut ids: Vec<_> = objects.iter().map(|o| o.physical_track_id).collect();
        ids.sort_unstable();
        if edges.is_empty() {
            return ids.into_iter().map(|id| BTreeSet::from([id])).collect();
        }
        let active: BTreeSet<_> = edges.keys().flatten().copied().collect();
        let mut groups: Groups = ids
            .iter()
            .filter(|id| active.contains(*id))
            .chain(ids.iter().filter(|id| !active.contains(*id)))
            .map(|&id| BTreeSet::from([id]))
            .collect();
        loop {
            let mut choices = vec![];
            for (i, a) in groups.iter().enumerate() {
                for (j, b) in groups.iter().enumerate().skip(i + 1) {
                    if a.len() + b.len() > 8 {
                        continue;
                    }
                    let cross: Vec<_> = a
                        .iter()
                        .flat_map(|&x| b.iter().map(move |&y| pair(x, y)))
                        .collect();
                    if cross.iter().all(|p| edges.contains_key(p)) {
                        let overlap = previous
                            .iter()
                            .any(|old| !old.is_disjoint(a) && !old.is_disjoint(b));
                        let score = cross.iter().map(|p| edges[p]).reduce(pmax).unwrap();
                        choices.push((
                            !overlap,
                            score,
                            *a.first().unwrap(),
                            *b.first().unwrap(),
                            i,
                            j,
                        ));
                    }
                }
            }
            if choices.is_empty() {
                break;
            }
            choices.sort_by(|a, b| {
                a.0.cmp(&b.0)
                    .then_with(|| cmp(a.1, b.1))
                    .then_with(|| (a.2, a.3, a.4, a.5).cmp(&(b.2, b.3, b.4, b.5)))
            });
            let chosen = choices[0];
            let removed = groups.remove(chosen.5);
            groups[chosen.4].extend(removed);
        }
        groups
    }
    fn choose_representatives<'a>(
        &mut self,
        groups: &Groups,
        by_pid: &BTreeMap<u32, &'a Object>,
        timestamp: i64,
        yaw: Option<f64>,
    ) -> Result<BTreeMap<Vec<u32>, &'a Object>, &'static str> {
        let multi: Vec<_> = groups.iter().filter(|g| g.len() > 1).collect();
        let mut claims = vec![];
        for (i, group) in multi.iter().enumerate() {
            for (j, prior) in self.representatives.iter().enumerate() {
                let overlap = group.intersection(&prior.members).count();
                if overlap > 0 {
                    let score = (10 * overlap
                        + 3 * usize::from(group.contains(&prior.representative_pid)))
                        as f64
                        + prior.age.min(1000) as f64 * 1e-5;
                    claims.push((
                        -score,
                        *group.first().unwrap(),
                        prior.representative_pid,
                        i,
                        j,
                    ));
                }
            }
        }
        claims.sort_by(|a, b| {
            cmp(a.0, b.0).then_with(|| (a.1, a.2, a.3, a.4).cmp(&(b.1, b.2, b.3, b.4)))
        });
        let (mut used_current, mut used_prior) = (BTreeSet::new(), BTreeSet::new());
        let mut matches = BTreeMap::new();
        for (_, _, _, i, j) in claims {
            if !used_current.contains(&i) && !used_prior.contains(&j) {
                matches.insert(i, j);
                used_current.insert(i);
                used_prior.insert(j);
            }
        }
        let mut states = vec![];
        let mut representatives = BTreeMap::new();
        for (i, group) in multi.into_iter().enumerate() {
            let members: Vec<_> = group.iter().map(|id| by_pid[id]).collect();
            let prior = matches.get(&i).map(|&j| &self.representatives[j]);
            let mut predicted = None;
            let mut median = 0.;
            if let Some(prior) = prior {
                let dt = (timestamp - prior.timestamp_ns) as f64 / 1e9;
                let angle = -yaw.filter(|v| v.is_finite()).unwrap_or(0.) * dt;
                let (co, si) = trig(angle)?;
                let dx = prior.d_rel.0 + prior.v_rel.0 * dt;
                predicted = Some((dx * co - prior.y_rel.0 * si, dx * si + prior.y_rel.0 * co));
            } else {
                let mut ordered: Vec<_> = members.iter().map(|o| o.d_rel).collect();
                ordered.sort_by(|a, b| cmp(*a, *b));
                let middle = ordered.len() / 2;
                median = if ordered.len() & 1 == 1 {
                    ordered[middle]
                } else {
                    (ordered[middle - 1] + ordered[middle]) * 0.5
                };
            }
            let cost = |o: &Object| {
                let continuity = if let Some(prior) = prior {
                    let (px, py) = predicted.unwrap();
                    (o.d_rel - px).abs()
                        + 0.5 * (o.y_rel - py).abs()
                        + 0.5 * (o.v_rel - prior.v_rel.0).abs()
                } else {
                    (o.d_rel - median).abs()
                };
                (
                    continuity,
                    prior.is_some_and(|p| o.physical_track_id != p.representative_pid),
                    !o.vision_supported,
                    !o.oem_selected,
                    std::cmp::Reverse(o.age_scans),
                    o.physical_track_id,
                )
            };
            let rep = members
                .into_iter()
                .min_by(|a, b| {
                    let ca = cost(a);
                    let cb = cost(b);
                    (!a.oem_selected)
                        .cmp(&!b.oem_selected)
                        .then_with(|| cmp(a.d_rel, b.d_rel))
                        .then_with(|| cmp(ca.0, cb.0))
                        .then_with(|| {
                            (ca.1, ca.2, ca.3, ca.4, ca.5).cmp(&(cb.1, cb.2, cb.3, cb.4, cb.5))
                        })
                })
                .unwrap();
            if let Some(prior) = prior {
                if rep.physical_track_id != prior.representative_pid {
                    self.representative_switches += 1;
                    let (px, py) = predicted.unwrap();
                    self.max_representative_jump[0].0 =
                        pmax(self.max_representative_jump[0].0, (rep.d_rel - px).abs());
                    self.max_representative_jump[1].0 =
                        pmax(self.max_representative_jump[1].0, (rep.y_rel - py).abs());
                    self.max_representative_jump[2].0 = pmax(
                        self.max_representative_jump[2].0,
                        (rep.v_rel - prior.v_rel.0).abs(),
                    );
                }
            }
            let key: Vec<_> = group.iter().copied().collect();
            representatives.insert(key, rep);
            states.push(Representative {
                members: group.clone(),
                representative_pid: rep.physical_track_id,
                timestamp_ns: timestamp,
                d_rel: Number(rep.d_rel),
                y_rel: Number(rep.y_rel),
                v_rel: Number(rep.v_rel),
                age: prior.map_or(1, |p| p.age + 1),
            });
        }
        self.representatives = states;
        Ok(representatives)
    }
    fn maturity(
        &self,
        members: &[u32],
        camera: (i64, i64),
        timestamp: i64,
        by_pid: &BTreeMap<u32, &Object>,
        yaw: Option<f64>,
        confirmation: (bool, Option<i64>),
    ) -> Result<History, &'static str> {
        let (episode, class) = camera;
        let (confirmed, confirm_ns) = confirmation;
        let mut age = 0;
        if let Some(prior) = self
            .histories
            .get(members)
            .filter(|p| p.cam_key == episode && !p.observations.is_empty())
        {
            let dt = (timestamp - prior.sample_ns) as f64 * 1e-9;
            if 0. < dt && dt <= 0.160 {
                let angle = -yaw.filter(|v| v.is_finite()).unwrap_or(0.) * dt;
                let (co, si) = trig(angle)?;
                let continuous = members.iter().zip(&prior.observations).all(
                    |(pid, &(Number(d), Number(y), Number(v)))| {
                        let o = by_pid[pid];
                        let x = d + v * dt;
                        (o.d_rel - (x * co - y * si)).abs() <= 1. + 2.5 * dt * dt
                            && (o.y_rel - (x * si + y * co)).abs()
                                <= 0.0625 + 2. * dt + 0.005 * o.d_rel.abs()
                            && (o.v_rel - v).abs() <= 0.25 + 5. * dt
                    },
                );
                if continuous {
                    age = if confirmed {
                        (prior.stable_intervals + 1).min(2)
                    } else {
                        prior.stable_intervals
                    };
                }
            }
        }
        let anchor = members
            .iter()
            .find(|id| by_pid[*id].oem_selected)
            .map_or(-1, |id| i64::from(*id));
        Ok(History {
            members: members.to_vec(),
            cam_key: episode,
            class_code: class,
            last_confirm_ns: confirm_ns.unwrap_or(timestamp),
            stable_intervals: age,
            sample_ns: timestamp,
            observations: members
                .iter()
                .map(|pid| {
                    let o = by_pid[pid];
                    (Number(o.d_rel), Number(o.y_rel), Number(o.v_rel))
                })
                .collect(),
            oem_anchor: anchor,
        })
    }
    fn footprint(
        geometry: &Geometry,
        associations: &Associations,
        by_pid: &BTreeMap<u32, &Object>,
        cameras: &Cameras,
    ) -> Associations {
        let mut out = BTreeMap::new();
        for (key, &(_, _, dv)) in geometry {
            let (a, b) = (by_pid[&key[0]], by_pid[&key[1]]);
            let (near, far) = if cmp(a.d_rel, b.d_rel)
                .then_with(|| a.physical_track_id.cmp(&b.physical_track_id))
                .is_le()
            {
                (a, b)
            } else {
                (b, a)
            };
            let Some(verdict) = associations.get(&near.physical_track_id) else {
                continue;
            };
            let Some(far_verdict) = associations.get(&far.physical_track_id) else {
                continue;
            };
            if verdict.0 != 1
                || far_verdict.0 != 0
                || out.contains_key(&far.physical_track_id)
                || dv > 0.5
            {
                continue;
            }
            let Some(c) = cameras.get(&verdict.1) else {
                continue;
            };
            if !(verdict.2 == 1 || (verdict.2 == 6 && c.width_m >= 2.25)) {
                continue;
            }
            let (_, neg) = long_window(range_span(far.d_rel), c.width_m);
            let long = c.long_m - far.d_rel;
            let lat = c.lat_m + far.y_rel;
            if !(-neg <= long
                && long < 0.
                && -long <= 12.
                && lat.abs() <= pmin(1.75, c.width_m * 0.5 + 0.5))
            {
                continue;
            }
            out.insert(far.physical_track_id, (1, verdict.1, verdict.2));
        }
        out
    }
    fn truck_continuous(
        prior: &TruckHistory,
        timestamp: i64,
        dd: f64,
        dy: f64,
        dv: f64,
        c: &CameraObject,
    ) -> bool {
        let dt = (timestamp - prior.last_ns) as f64 * 1e-9;
        0. < dt
            && dt <= 160_000_000_f64 * 1e-9
            && prior.episode == c.episode
            && prior.camera_id == c.obj_id
            && (dd - prior.dd.0).abs() <= 0.50
            && (dy - prior.dy.0).abs() <= 0.375
            && (dv - prior.dv.0).abs() <= 0.25
            && (c.long_m - (prior.camera_d.0 + prior.camera_v.0 * dt)).abs() <= 0.75
            && (c.lat_m - prior.camera_y.0).abs() <= 0.125
            && (c.vrel_mps - prior.camera_v.0).abs() <= 0.50
            && (c.width_m - prior.camera_width.0).abs() <= 0.10
    }
    fn truck_near_miss(
        obj: &Object,
        anchor: &CameraObject,
        cameras: &[CameraObject],
        count: usize,
    ) -> Result<bool, &'static str> {
        let bearing = (-obj.y_rel).atan2(pmax(obj.d_rel, 0.5));
        let span = range_span(obj.d_rel);
        let mut anchor_cost = f64::INFINITY;
        let mut other_cost = f64::INFINITY;
        let mut gate = false;
        for i in 0..count {
            let c = cameras.get(i).ok_or("list index out of range")?;
            let (pos, neg) = long_window(span, c.width_m);
            let long = c.long_m - obj.d_rel;
            let lat = c.lat_m + obj.y_rel;
            let lo = pmin(c.angle_left, c.angle_right);
            let hi = pmax(c.angle_left, c.angle_right);
            let outside = pmax(lo - bearing, bearing - hi);
            let half = (hi - lo) * 0.5;
            let bearing_normal = (half + outside) / pmax(half + 0.020, 1e-6);
            let cost = 4. * bearing_normal
                + 2. * (c.vrel_mps - obj.v_rel).abs() / 3.
                + lat.abs() / 1.75
                + 0.5 * long.abs() / pmax(pos + neg, 1e-6);
            if c.obj_id == anchor.obj_id && c.episode == anchor.episode {
                anchor_cost = cost;
                gate = -neg <= long
                    && long <= pos
                    && lat.abs() <= 1.75
                    && 0.020 < outside
                    && outside <= 0.020 + 0.010;
            } else {
                other_cost = pmin(other_cost, cost)
            }
        }
        Ok(gate && anchor_cost + 0.15 <= other_cost)
    }
    fn truck_history(
        key: Vec<u32>,
        timestamp: i64,
        confirmations: usize,
        geometry: (f64, f64, f64),
        c: &CameraObject,
        recovery: usize,
    ) -> TruckHistory {
        let (dd, dy, dv) = geometry;
        TruckHistory {
            members: key,
            camera_id: c.obj_id,
            episode: c.episode,
            last_ns: timestamp,
            confirmations,
            dd: Number(dd),
            dy: Number(dy),
            dv: Number(dv),
            camera_d: Number(c.long_m),
            camera_y: Number(c.lat_m),
            camera_v: Number(c.vrel_mps),
            camera_width: Number(c.width_m),
            recovery_age: recovery,
        }
    }
    fn truck_strict(
        &mut self,
        timestamp: i64,
        geometry: &Geometry,
        associations: &Associations,
        by_pid: &BTreeMap<u32, &Object>,
        camera_by_episode: &Cameras,
        camera_view: (&[CameraObject], usize),
    ) -> Result<BTreeMap<Vec<u32>, f64>, &'static str> {
        let (camera_objects, count) = camera_view;
        let mut candidates = vec![];
        for (key, &(dd, dy, dv)) in geometry {
            let aa = associations.get(&key[0]).copied().unwrap_or((0, -1, -1));
            let ab = associations.get(&key[1]).copied().unwrap_or((0, -1, -1));
            if !(aa.0 == 1 && ab.0 == 1 && aa.1 == ab.1 && aa.1 >= 0 && aa.2 == 6 && ab.2 == 6) {
                continue;
            }
            let Some(c) = camera_by_episode.get(&aa.1) else {
                continue;
            };
            let fresh = self
                .last_camera_ns
                .is_some_and(|ns| (0..=160_000_000).contains(&(timestamp - ns)))
                && by_pid[&key[0]].timestamp_ns == timestamp
                && by_pid[&key[1]].timestamp_ns == timestamp;
            if !(fresh
                && c.width_m >= 2.25
                && (5.50..=12.).contains(&dd)
                && dy <= 0.875
                && dv <= 0.50)
            {
                continue;
            }
            candidates.push((dd / 12. + dy / 1.5 + dv / 1.5, key.clone(), dd, dy, dv, c));
        }
        candidates.sort_by(|a, b| cmp(a.0, b.0).then_with(|| a.1.cmp(&b.1)));
        let mut history = BTreeMap::new();
        let mut strict = BTreeMap::new();
        for (score, key, dd, dy, dv, c) in candidates.into_iter().take(16) {
            let confirmations = self
                .truck_pair_histories
                .get(&key)
                .filter(|p| Self::truck_continuous(p, timestamp, dd, dy, dv, c))
                .map_or(1, |p| p.confirmations + 1);
            history.insert(
                key.clone(),
                Self::truck_history(key.clone(), timestamp, confirmations, (dd, dy, dv), c, 0),
            );
            if confirmations >= 3 {
                strict.insert(key, score);
            }
        }
        let mut recovery = vec![];
        if history.len() < 16 {
            for (key, prior) in &self.truck_pair_histories {
                if history.contains_key(key)
                    || !geometry.contains_key(key)
                    || prior.confirmations < 3
                {
                    continue;
                }
                let prior_rep = self.representatives.iter().find(|rep| {
                    rep.members.len() == key.len()
                        && key.iter().all(|pid| rep.members.contains(pid))
                });
                if prior_rep.is_none_or(|r| !by_pid.contains_key(&r.representative_pid)) {
                    continue;
                }
                let verdicts = [
                    associations.get(&key[0]).copied().unwrap_or((0, -1, -1)),
                    associations.get(&key[1]).copied().unwrap_or((0, -1, -1)),
                ];
                let assigned: Vec<_> = verdicts
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| v.0 == 1)
                    .map(|(i, _)| i)
                    .collect();
                if assigned.len() != 1 {
                    continue;
                }
                let ai = assigned[0];
                let missing = 1 - ai;
                let (anchor, missing_verdict) = (verdicts[ai], verdicts[missing]);
                if missing_verdict.0 != 0 || anchor.1 != prior.episode || anchor.2 != 6 {
                    continue;
                }
                let Some(c) = camera_by_episode.get(&anchor.1) else {
                    continue;
                };
                let (dd, dy, dv) = geometry[key];
                let fresh = c.obj_id == prior.camera_id
                    && self
                        .last_camera_ns
                        .is_some_and(|ns| (0..=160_000_000).contains(&(timestamp - ns)))
                    && by_pid[&key[0]].timestamp_ns == timestamp
                    && by_pid[&key[1]].timestamp_ns == timestamp;
                let absolute = fresh
                    && c.width_m >= 2.25
                    && (5.50..=12.).contains(&dd)
                    && dy <= 0.875
                    && dv <= 0.50;
                let age = prior.recovery_age + 1;
                if !absolute
                    || age > 2
                    || !Self::truck_continuous(prior, timestamp, dd, dy, dv, c)
                    || !Self::truck_near_miss(by_pid[&key[missing]], c, camera_objects, count)?
                {
                    continue;
                }
                recovery.push((
                    dd / 12. + dy / 1.5 + dv / 1.5,
                    key.clone(),
                    dd,
                    dy,
                    dv,
                    c,
                    prior.confirmations,
                    age,
                ));
            }
        }
        recovery.sort_by(|a, b| cmp(a.0, b.0).then_with(|| a.1.cmp(&b.1)));
        let mut recovered = 0;
        let capacity = 16 - history.len();
        for (score, key, dd, dy, dv, c, confirmations, age) in recovery.into_iter().take(capacity) {
            history.insert(
                key.clone(),
                Self::truck_history(key.clone(), timestamp, confirmations, (dd, dy, dv), c, age),
            );
            strict.insert(key, score);
            recovered += 1;
        }
        self.truck_pair_histories = history;
        self.max_truck_pair_state = self
            .max_truck_pair_state
            .max(self.truck_pair_histories.len());
        self.last_truck_edge_count = strict.len();
        self.last_truck_recovery_count = recovered;
        Ok(strict)
    }
    pub fn update(
        &mut self,
        timestamp: i64,
        objects: &[Object],
        ego: f64,
        yaw: Option<f64>,
        snapshot: Option<Snapshot<'_>>,
    ) -> Result<(), &'static str> {
        if self.mode == 0 {
            return Ok(());
        }
        let prior_maturity = self.histories.clone();
        let prior_mature: BTreeSet<_> = self.mature_groups.iter().cloned().collect();
        self.mature_groups.clear();
        let clock_reset = self.last_ns.is_some_and(|last| timestamp <= last);
        if self
            .last_ns
            .is_some_and(|last| timestamp - last > 160_000_000)
        {
            self.histories.clear();
            self.truck_pair_histories.clear();
            self.representatives.clear();
            self.curve_reacquire_histories.clear();
        } else if clock_reset {
            self.curve_reacquire_histories.clear();
        }
        self.last_ns = Some(timestamp);
        let by_pid: BTreeMap<_, _> = objects.iter().map(|o| (o.physical_track_id, o)).collect();
        let mut ordered: Vec<_> = objects.iter().collect();
        ordered.sort_by(|a, b| {
            cmp(a.d_rel, b.d_rel).then_with(|| a.physical_track_id.cmp(&b.physical_track_id))
        });
        let mut geometry = Geometry::new();
        let mut nodes = BTreeSet::new();
        for (i, a) in ordered.iter().enumerate() {
            for b in &ordered[i + 1..] {
                let dd = b.d_rel - a.d_rel;
                if dd > 12. {
                    break;
                }
                if dd <= 3. {
                    continue;
                }
                let (dd, dy, dv, okay) = Self::geometry(a, b, ego, yaw);
                if !okay {
                    continue;
                }
                let key = pair(a.physical_track_id, b.physical_track_id);
                nodes.extend(key.iter().copied());
                geometry.insert(key, (dd, dy, dv));
            }
        }
        self.last_candidate_count = geometry.len();
        self.last_camera_ns = snapshot.map(|s| s.complete_ns);
        let mut associations = Associations::new();
        let mut camera_by_episode = Cameras::new();
        let mut camera_objects: &[CameraObject] = &[];
        let mut camera_count = 0;
        if let Some(snapshot) = snapshot {
            camera_objects = snapshot.objects;
            camera_count = snapshot.count;
            for i in 0..camera_count {
                let c = camera_objects.get(i).ok_or("list index out of range")?;
                camera_by_episode.insert(c.episode, c.clone());
            }
            self.curve_prepare(timestamp, &by_pid, camera_objects, camera_count)?;
            let baseline_nodes: Vec<_> = if self.curve_reacquire_mode != 0 {
                by_pid.keys().copied().collect()
            } else {
                nodes.iter().copied().collect()
            };
            let mut baseline_associations = Associations::new();
            let mut reacquired = vec![];
            let mut would = vec![];
            for pid in baseline_nodes {
                let obj = by_pid[&pid];
                let baseline = associate(
                    Measurement {
                        d_rel: obj.d_rel,
                        y_rel: obj.y_rel,
                        v_rel: obj.v_rel,
                    },
                    camera_objects,
                    camera_count,
                )?;
                baseline_associations.insert(pid, baseline);
                if nodes.contains(&pid) {
                    associations.insert(pid, baseline);
                }
                if baseline.0 == 0 && self.curve_reacquire_mode != 0 {
                    let candidate =
                        self.curve_reacquire(obj, camera_objects, camera_count, timestamp, yaw)?;
                    if candidate.0 == 1 {
                        let c = &camera_by_episode[&candidate.1];
                        would.push((pid, c.obj_id, c.episode));
                        if self.curve_reacquire_mode == 2 {
                            associations.insert(pid, candidate);
                            reacquired.push((pid, c.obj_id, c.episode));
                            self.curve_reacquire_successes += 1;
                        }
                    }
                }
            }
            self.curve_update(timestamp, &baseline_associations, &camera_by_episode);
            self.last_baseline_associations = baseline_associations;
            reacquired.sort_unstable();
            would.sort_unstable();
            self.last_curve_reacquire = reacquired;
            self.last_curve_reacquire_would = would;
        } else {
            self.curve_reacquire_histories.clear();
            self.last_baseline_associations.clear();
            self.last_curve_reacquire.clear();
            self.last_curve_reacquire_would.clear();
        }
        self.last_association_count = associations.len();
        self.last_associations = associations.clone();
        self.last_camera_by_episode = camera_by_episode.clone();
        self.last_v_ego = Number(ego);
        self.last_yaw_rate = yaw.map(Number);
        let footprint = Self::footprint(&geometry, &associations, &by_pid, &camera_by_episode);
        self.last_footprint_associations = footprint.keys().copied().collect();
        associations.extend(footprint);
        let mut strict = BTreeMap::new();
        let mut classes = BTreeMap::new();
        let mut episodes = BTreeMap::new();
        for (key, &(dd, dy, dv)) in &geometry {
            let aa = associations.get(&key[0]).copied().unwrap_or((0, -1, -1));
            let ab = associations.get(&key[1]).copied().unwrap_or((0, -1, -1));
            if aa.0 == 1 && ab.0 == 1 && aa.1 == ab.1 && aa.1 >= 0 && aa.2 == 1 && ab.2 == 1 {
                strict.insert(key.clone(), dd / 12. + dy / 1.5 + dv / 1.5);
                classes.insert(key.clone(), 1);
                episodes.insert(key.clone(), aa.1);
            }
        }
        let truck = self.truck_strict(
            timestamp,
            &geometry,
            &associations,
            &by_pid,
            &camera_by_episode,
            (camera_objects, camera_count),
        )?;
        for (key, score) in truck {
            classes.insert(key.clone(), 6);
            episodes.insert(key.clone(), self.truck_pair_histories[&key].episode);
            strict.insert(key, score);
        }
        let mut edges = strict.clone();
        let mut next_history = BTreeMap::new();
        let mut coast = vec![];
        for (members, history) in &self.histories {
            let pairs = pairs(members);
            let verdicts: Vec<_> = members
                .iter()
                .map(|pid| associations.get(pid).copied().unwrap_or((0, -1, -1)))
                .collect();
            let assigned: Vec<_> = verdicts.iter().filter(|v| v.0 == 1).collect();
            let okay = members.iter().all(|pid| by_pid.contains_key(pid))
                && pairs.iter().all(|p| geometry.contains_key(p))
                && timestamp - history.last_confirm_ns <= 250_000_000
                && !verdicts.iter().any(|v| v.0 == 2)
                && !assigned
                    .iter()
                    .any(|v| v.1 != history.cam_key || v.2 != history.class_code)
                && assigned.iter().any(|v| v.1 == history.cam_key);
            if !okay {
                continue;
            }
            if !pairs.iter().all(|p| strict.contains_key(p)) {
                let members_set: BTreeSet<_> = members.iter().copied().collect();
                edges.retain(|pair, _| {
                    let p: BTreeSet<_> = pair.iter().copied().collect();
                    p.is_disjoint(&members_set) || p.is_subset(&members_set)
                });
                for pair in pairs {
                    let (dd, dy, dv) = geometry[&pair];
                    edges.insert(pair, dd / 12. + dy / 1.5 + dv / 1.5);
                }
                coast.push(members.clone());
                next_history.insert(members.clone(), history.clone());
            }
        }
        self.last_coast_count = coast.len();
        let previous: Vec<_> = self
            .representatives
            .iter()
            .map(|r| r.members.clone())
            .collect();
        let groups = Self::complete_link(objects, &edges, &previous);
        let representatives = self.choose_representatives(&groups, &by_pid, timestamp, yaw)?;
        let mut mature = vec![];
        for group in &groups {
            if group.len() < 2 {
                continue;
            }
            let members: Vec<_> = group.iter().copied().collect();
            let pairs = pairs(&members);
            if pairs.iter().all(|p| strict.contains_key(p)) {
                let episode = episodes[&pairs[0]];
                let class = classes[&pairs[0]];
                let fresh = self
                    .last_camera_ns
                    .is_some_and(|ns| (0..=160_000_000).contains(&(timestamp - ns)))
                    && members.iter().all(|p| by_pid[p].timestamp_ns == timestamp);
                if self.mode == 3 && fresh {
                    let state = self.maturity(
                        &members, (episode, class), timestamp, &by_pid, yaw, (true, None),
                    )?;
                    let prior = self.histories.get(&members);
                    let mut required = 2;
                    if prior
                        .is_some_and(|p| state.oem_anchor >= 0 && p.oem_anchor == state.oem_anchor)
                    {
                        required = 1;
                    }
                    if class == 1
                        && representatives.get(&members).is_some_and(|rep| {
                            rep.d_rel
                                <= members
                                    .iter()
                                    .map(|p| by_pid[p].d_rel)
                                    .reduce(pmin)
                                    .unwrap()
                        })
                    {
                        required = required.min(1);
                    }
                    if state.stable_intervals >= required {
                        mature.push(members.clone());
                    }
                    next_history.insert(members, state);
                } else {
                    next_history.insert(
                        members.clone(),
                        History {
                            members,
                            cam_key: episode,
                            class_code: class,
                            last_confirm_ns: timestamp,
                            stable_intervals: 0,
                            sample_ns: 0,
                            observations: vec![],
                            oem_anchor: -1,
                        },
                    );
                }
            } else if let Some(state) = next_history.get(&members) {
                if self.mode == 3
                    && !state.observations.is_empty()
                    && state.stable_intervals > 0
                    && timestamp - state.last_confirm_ns <= 200_000_000
                {
                    let coasted = self.maturity(
                        &members,
                        (state.cam_key, state.class_code),
                        timestamp,
                        &by_pid,
                        yaw,
                        (false, Some(state.last_confirm_ns)),
                    )?;
                    let required =
                        if coasted.oem_anchor >= 0 && state.oem_anchor == coasted.oem_anchor {
                            1
                        } else {
                            2
                        };
                    if prior_mature.contains(&members)
                        && coasted.stable_intervals >= required
                        && members.iter().all(|p| by_pid[p].timestamp_ns == timestamp)
                        && representatives.get(&members).is_some_and(|rep| {
                            rep.d_rel
                                <= members
                                    .iter()
                                    .map(|p| by_pid[p].d_rel)
                                    .reduce(pmin)
                                    .unwrap()
                        })
                    {
                        mature.push(members.clone());
                    }
                    next_history.insert(members, coasted);
                } else {
                    next_history.insert(
                        members.clone(),
                        History {
                            members,
                            cam_key: state.cam_key,
                            class_code: state.class_code,
                            last_confirm_ns: state.last_confirm_ns,
                            stable_intervals: 0,
                            sample_ns: 0,
                            observations: vec![],
                            oem_anchor: -1,
                        },
                    );
                }
            } else {
                return Err("E2 coast created an unconfirmed extended member set");
            }
        }
        self.last_maturity_resets = prior_maturity
            .iter()
            .filter(|(key, old)| {
                !old.observations.is_empty()
                    && next_history
                        .get(*key)
                        .is_none_or(|new| new.observations.is_empty() || new.stable_intervals == 0)
            })
            .count();
        self.maturity_resets += self.last_maturity_resets;
        self.mature_groups = mature;
        self.histories = next_history;
        self.max_state_count = self.max_state_count.max(self.histories.len());
        self.last_groups = groups
            .into_iter()
            .filter(|g| g.len() > 1)
            .map(|g| g.into_iter().collect())
            .collect();
        self.last_groups.sort();
        Ok(())
    }
}
