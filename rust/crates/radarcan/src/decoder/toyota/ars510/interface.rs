//! Native ARS510 lifecycle, causal input caches and source-ordered point publication.
use super::{
    config::NativeInterfaceConfig,
    fusion::Fusion,
    objects::{decode_native_slot, NativeObject},
    record, support,
    tracks::NativeTrackIdAssigner,
    transport::{CompletedRecord, SegmentedRecordAssembler, StrictId80RecordAssembler},
};
use crate::{
    scalar::{divide, float_sum, maximum, square},
    Error,
};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
};
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum Assembler {
    Strict(StrictId80RecordAssembler),
    Base(SegmentedRecordAssembler),
}
impl Assembler {
    pub fn push(&mut self, time: f64, data: &[u8]) -> Option<CompletedRecord> {
        match self {
            Self::Strict(a) => a.push(time, data),
            Self::Base(a) => a.push(time, data),
        }
    }
}
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Number(
    #[serde(
        serialize_with = "crate::scalar::serialize_float",
        deserialize_with = "crate::scalar::deserialize_float"
    )]
    pub f64,
);
#[derive(serde::Serialize, serde::Deserialize)]
pub struct NativeInterface {
    #[serde(flatten)]
    pub math: Fusion,
    pub _assembler: Assembler,
    pub _tracks: NativeTrackIdAssigner,
    _v_ego: Option<f64>,
    _v_ego_time_s: Option<f64>,
    _a_ego: Option<f64>,
    _yaw: Option<(f64, f64)>,
    pub crc_failures: u64,
    pub records: u64,
    pub settling_suppressed: u64,
    pub relinks: u64,
    pub unresolved_suppressed: u64,
    #[serde(with = "super::float_maps")]
    _first: BTreeMap<u64, Vec<f64>>,
    #[serde(with = "super::float_maps")]
    _last: BTreeMap<u64, Vec<f64>>,
    _out_id: BTreeMap<u64, u64>,
    _claimed: BTreeSet<u64>,
    _acc_vrel: Option<(f64, f64)>,
    _acc_pos: Option<(f64, f64, f64)>,
    _acc_assoc: Option<(u64, f64, f64)>,
    pub path_gated: u64,
    _summary_hist: BTreeMap<u32, Vec<(f64, f64)>>,
    _summary_assoc: BTreeMap<u32, Option<u64>>,
    _summary_y: BTreeMap<u32, f64>,
    _guard_state: BTreeMap<u64, (f64, Number, Option<f64>, bool)>,
    _guard_gen: BTreeMap<u64, u64>,
    pub guard_rejected: u64,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Point {
    pub track_id: u64,
    pub d_rel: Number,
    pub y_rel: Number,
    pub v_rel: Number,
    pub a_rel: Number,
    pub yv_rel: Number,
    pub measured: bool,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}
#[derive(serde::Serialize)]
pub struct Metadata {
    pub slot: usize,
    pub age: i64,
    pub v_long_ground: Number,
    pub native_id: u64,
    pub existence_pct: Option<i64>,
    pub move_state: i64,
    pub oncoming_flag: bool,
    pub vrel_status: &'static str,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    #[serde(rename = "time_s")]
    pub time_s: Number,
    pub radar_data: RadarData,
}
#[derive(serde::Serialize)]
pub struct RadarData {
    pub points: Vec<Point>,
    pub errors: BTreeMap<&'static str, bool>,
}
type Decoded = Vec<(usize, NativeObject, u64)>;
impl NativeInterface {
    pub fn new(config: NativeInterfaceConfig) -> Self {
        Self {
            math: Fusion::new(config),
            _assembler: Assembler::Base(SegmentedRecordAssembler::id80()),
            _tracks: NativeTrackIdAssigner::default(),
            _v_ego: None,
            _v_ego_time_s: None,
            _a_ego: None,
            _yaw: None,
            crc_failures: 0,
            records: 0,
            settling_suppressed: 0,
            relinks: 0,
            unresolved_suppressed: 0,
            _first: BTreeMap::new(),
            _last: BTreeMap::new(),
            _out_id: BTreeMap::new(),
            _claimed: BTreeSet::new(),
            _acc_vrel: None,
            _acc_pos: None,
            _acc_assoc: None,
            path_gated: 0,
            _summary_hist: [(0x192, vec![]), (0x194, vec![])].into(),
            _summary_assoc: [(0x192, None), (0x194, None)].into(),
            _summary_y: [(0x192, 0.), (0x194, 0.)].into(),
            _guard_state: BTreeMap::new(),
            _guard_gen: BTreeMap::new(),
            guard_rejected: 0,
        }
    }
    pub fn set_ego_speed(&mut self, speed: f64, time: f64) {
        if speed.is_finite() && time.is_finite() {
            self._a_ego = match (self._v_ego, self._v_ego_time_s) {
                (Some(old), Some(t)) if 0. < time - t && time - t < 0.5 => {
                    let dt = time - t;
                    let a = (speed - old) / dt;
                    let k = 1. - (-dt / 0.3).exp();
                    Some(self._a_ego.map_or(a, |old| old + k * (a - old)))
                }
                _ => None,
            };
            self._v_ego = Some(speed);
            self._v_ego_time_s = Some(time);
        }
    }
    fn ego(&self, time: f64) -> Option<f64> {
        match (self._v_ego, self._v_ego_time_s) {
            (Some(v), Some(t))
                if (time - t)
                    .abs()
                    .partial_cmp(&self.math.config.max_ego_speed_age_s)
                    != Some(Ordering::Greater) =>
            {
                Some(v)
            }
            _ => None,
        }
    }
    fn yaw(&self, time: f64) -> Option<f64> {
        self._yaw
            .filter(|(t, _)| {
                (time - t)
                    .abs()
                    .partial_cmp(&self.math.config.max_ego_speed_age_s)
                    != Some(Ordering::Greater)
            })
            .map(|(_, v)| v)
    }
    pub fn update_frame(
        &mut self,
        time: f64,
        bus: i64,
        address: u32,
        data: &[u8],
    ) -> Result<Option<Output>, Error> {
        let c = &self.math.config;
        if bus == c.ego_speed_bus && address == 0x24 {
            if let Some(yaw) = support::parse_toyota_yaw_rate(data) {
                self._yaw = Some((time, yaw));
            }
            return Ok(None);
        }
        if bus == c.ego_speed_bus && address == 0xb4 {
            if let Some(v) = support::parse_toyota_speed_mps(data) {
                self.set_ego_speed(v, time);
            }
            return Ok(None);
        }
        if bus == c.radar_bus && [0x192, 0x194].contains(&address) {
            if c.fused_speed_filter && c.summary_sigma_mps > 0. {
                self.summary_update(address, time, data);
            }
            return Ok(None);
        }
        if bus == c.radar_bus && [0x235, 0x237].contains(&address) {
            let available = data.len() == 8
                && if address == 0x235 {
                    data[1] & 4 != 0
                } else {
                    data[2..] != [0, 0x3e, 0x80, 0, 0, 0]
                };
            if !available {
                self._acc_vrel = None;
                self._acc_pos = None;
            } else if address == 0x235 {
                if let Some(v) = support::parse_acc_target_vrel(data) {
                    self._acc_vrel = Some((time, v));
                }
            } else if let Some((x, y)) = support::parse_acc_target_position(data) {
                self._acc_pos = Some((time, x, y));
            }
            return Ok(None);
        }
        if bus != c.radar_bus || address != 0x80 {
            return Ok(None);
        }
        let Some(record) = self._assembler.push(time, data) else {
            return Ok(None);
        };
        if !record::id80_crc_ok(&record.payload) {
            self.crc_failures += 1;
            return Ok(None);
        }
        self.records += 1;
        self.payload(record.time_s, &record.payload).map(Some)
    }
    fn matches(&self, first: &[f64], id: u64) -> bool {
        let last = &self._last[&id];
        let gap = first[0] - last[0];
        if last[4] < 30. || !(0. < gap && gap <= self.math.config.relink_max_gap_s) {
            return false;
        }
        let predicted = last[1] + if last[3].is_finite() { last[3] } else { 0. } * gap;
        (first[1] - predicted).abs() <= maximum(3., 0.10 * last[1]) + gap
            && (first[2] - last[2]).abs() <= 1.2 + 0.5 * gap
    }
    fn relink(&mut self, id: u64, time: f64, seen: &BTreeSet<u64>) -> u64 {
        let c = &self.math.config;
        if c.relink_max_gap_s <= 0. || c.min_publish_age < 6 || !self._first.contains_key(&id) {
            return id;
        }
        let candidates: Vec<_> = self
            ._last
            .iter()
            .filter(|(k, v)| {
                !seen.contains(k)
                    && **k != id
                    && !self._claimed.contains(k)
                    && time - v[0] > self._tracks.max_gap_s
                    && self.matches(&self._first[&id], **k)
            })
            .map(|(k, _)| *k)
            .collect();
        if candidates.len() != 1 {
            return id;
        }
        let lost = candidates[0];
        self._claimed.insert(lost);
        if self._last.keys().any(|other| {
            seen.contains(other)
                && *other != id
                && !self._out_id.contains_key(other)
                && self
                    ._first
                    .get(other)
                    .is_some_and(|first| self.matches(first, lost))
        }) {
            return id;
        }
        self.relinks += 1;
        self._out_id.get(&lost).copied().unwrap_or(lost)
    }
    fn acc_match(&mut self, time: f64, decoded: &Decoded) -> Result<(Option<u64>, f64), Error> {
        let c = &self.math.config;
        let empty = (None, f64::NAN);
        let (Some((tv, v)), Some((tp, x, y))) = (self._acc_vrel, self._acc_pos) else {
            self._acc_assoc = None;
            return Ok(empty);
        };
        if !c.fused_speed_filter
            || (time - tv).abs() > c.acc_target_max_age_s
            || (time - tp).abs() > c.acc_target_max_age_s
        {
            self._acc_assoc = None;
            return Ok(empty);
        }
        let range = maximum(c.acc_match_range_m, c.acc_match_range_frac * x);
        let mut costs = vec![];
        for (_, o, id) in decoded
            .iter()
            .filter(|(_, o, _)| o.geometry_valid && o.lateral_valid)
        {
            costs.push((
                divide((o.d_rel - x).abs(), range)? + (o.y_rel - y).abs() / 0.5,
                *id,
                o.age,
            ));
        }
        costs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        if let Some((id, oldx, oldy)) = self._acc_assoc {
            if costs
                .iter()
                .any(|(cost, key, _)| *key == id && *cost < c.acc_sticky_max_cost)
                && (x - oldx).abs() <= c.acc_sticky_jump_m
                && (y - oldy).abs() <= 1.
            {
                self._acc_assoc = Some((id, x, y));
                return Ok((Some(id), v));
            }
            self._acc_assoc = None;
        }
        if costs.is_empty()
            || costs[0].0 >= 1.
            || costs[0].2 < c.acc_match_min_age
            || costs.len() > 1 && costs[1].0 - costs[0].0 <= 1.
        {
            return Ok(empty);
        }
        let id = costs[0].1;
        self._acc_assoc = Some((id, x, y));
        Ok((Some(id), v))
    }
    fn summary_update(&mut self, address: u32, time: f64, data: &[u8]) {
        let hist = self
            ._summary_hist
            .get_mut(&address)
            .expect("summary address");
        let Some(word) = support::parse_0x192(data) else {
            hist.clear();
            self._summary_assoc.insert(address, None);
            return;
        };
        let (scale, offset) = self.math.config.summary_scales[usize::from(address != 0x192)];
        let x = word.range_code13 as f64 * scale + offset;
        if hist
            .last()
            .is_some_and(|(t, p)| time - t > 0.3 || (x - p).abs() > 5.)
        {
            hist.clear();
            self._summary_assoc.insert(address, None);
        }
        hist.push((time, x));
        self._summary_y.insert(address, word.y_rel());
        let first = hist
            .iter()
            .take_while(|(t, _)| *t < time - self.math.config.summary_window_s)
            .count();
        hist.drain(..first);
    }
    fn summary_speed(&self, address: u32, time: f64) -> Result<Option<(f64, f64)>, Error> {
        let h = &self._summary_hist[&address];
        let window = self.math.config.summary_window_s;
        if h.len() < 8 || h[h.len() - 1].0 - h[0].0 < 0.7 * window || time - h[h.len() - 1].0 > 0.3
        {
            return Ok(None);
        }
        let mt = float_sum(h.iter().map(|(t, _)| *t)) / h.len() as f64;
        let mx = float_sum(h.iter().map(|(_, x)| *x)) / h.len() as f64;
        let den = float_sum(
            h.iter()
                .map(|(t, _)| square(t - mt))
                .collect::<Result<Vec<_>, _>>()?,
        );
        if den > 0. {
            Ok(Some((
                h[h.len() - 1].1,
                float_sum(h.iter().map(|(t, x)| (t - mt) * (x - mx))) / den,
            )))
        } else {
            Ok(None)
        }
    }
    fn summary_match(
        &mut self,
        time: f64,
        decoded: &Decoded,
        ego: Option<f64>,
        acc: Option<u64>,
    ) -> Result<BTreeMap<u64, f64>, Error> {
        let mut out = BTreeMap::new();
        if ego.is_none() {
            return Ok(out);
        }
        let tracks: BTreeMap<_, _> = decoded
            .iter()
            .filter(|(_, o, _)| o.geometry_valid && o.lateral_valid && o.age >= 20)
            .map(|(_, o, id)| (*id, o))
            .collect();
        for address in [0x192, 0x194] {
            let Some((x, v)) = self.summary_speed(address, time)? else {
                self._summary_assoc.insert(address, None);
                continue;
            };
            let y = self._summary_y[&address];
            let c = &self.math.config;
            let mut id = self._summary_assoc[&address];
            if id.is_some_and(|id| {
                tracks.get(&id).is_none_or(|o| {
                    (o.d_rel - x).abs() > maximum(8., 0.25 * x)
                        || (o.y_rel - y).abs() > 2. * c.summary_match_lat_m
                })
            }) {
                id = None;
            }
            if id.is_none() {
                let mut candidates: Vec<_> = tracks
                    .iter()
                    .filter(|(_, o)| {
                        (o.y_rel - y).abs() < c.summary_match_lat_m
                            && (o.d_rel - x).abs() < maximum(5., 0.15 * x)
                    })
                    .map(|(id, o)| ((o.d_rel - x).abs(), *id))
                    .collect();
                candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                if candidates.len() == 1
                    || candidates.len() > 1 && candidates[1].0 - candidates[0].0 > 3.
                {
                    id = Some(candidates[0].1);
                }
            }
            self._summary_assoc.insert(address, id);
            if let Some(id) = id.filter(|id| Some(*id) != acc) {
                out.entry(id).or_insert(v);
            }
        }
        Ok(out)
    }
    fn guard(&mut self, id: u64, time: f64, v: f64, code: i64, age: i64) -> bool {
        let c = &self.math.config;
        let sat = c.drop_saturated_codes && (code >= 1023 || code == 0);
        let previous = self
            ._guard_state
            .get(&id)
            .copied()
            .filter(|s| (time - s.0).partial_cmp(&0.5) != Some(Ordering::Greater));
        let Some(mut s) = previous else {
            if age < c.guard_min_age {
                self._guard_state.remove(&id);
                return false;
            }
            self._guard_state.insert(
                id,
                if sat {
                    (time, Number(f64::NAN), Some(time), true)
                } else {
                    (time, Number(v), None, false)
                },
            );
            self.guard_rejected += u64::from(sat);
            return sat;
        };
        s.0 = time;
        let bad = sat || s.1 .0.is_finite() && s.3 && (v - s.1 .0).abs() > c.sat_recover_mps;
        let rejected = if !bad {
            if s.2.is_some() {
                *self._guard_gen.entry(id).or_default() += 1;
                s.2 = None;
                s.3 = false;
            }
            s.1 = Number(v);
            false
        } else {
            s.2.get_or_insert(time);
            if sat {
                s.3 = true;
                self.guard_rejected += 1;
                true
            } else if time - s.2.unwrap() > c.guard_hold_s {
                *self._guard_gen.entry(id).or_default() += 1;
                s.1 = Number(v);
                s.2 = None;
                s.3 = false;
                false
            } else {
                self.guard_rejected += 1;
                true
            }
        };
        self._guard_state.insert(id, s);
        rejected
    }
    pub fn payload(&mut self, time: f64, record: &[u8]) -> Result<Output, Error> {
        let ego = self.ego(time);
        let mut decoded = vec![];
        for (slot, segment) in record::occupied_slots(record) {
            let object = decode_native_slot(slot as i64, segment).map_err(Error::ArsValue)?;
            let id = self
                ._tracks
                .update(time, slot as i64, object.age)
                .map_err(|s| Error::ArsValue(s.into()))?;
            decoded.push((slot, object, id));
        }
        let seen = decoded.iter().map(|(_, _, id)| *id).collect();
        let (acc, vacc) = self.acc_match(time, &decoded)?;
        let summaries =
            if self.math.config.fused_speed_filter && self.math.config.summary_sigma_mps > 0. {
                self.summary_match(time, &decoded, ego, acc)?
            } else {
                BTreeMap::new()
            };
        let mut points = vec![];
        for (slot, o, id) in decoded {
            if !o.geometry_valid || !o.lateral_valid {
                continue;
            }
            if self.math.config.drop_saturated_codes
                && self.guard(id, time, o.v_long_ground, o.vel_code, o.age)
            {
                continue;
            }
            let (ground, std) =
                if let Some(ego) = ego.filter(|_| self.math.config.fused_speed_filter) {
                    let mut extra = vec![];
                    if Some(id) == acc {
                        extra.push((vacc + ego, self.math.config.acc_sigma_mps));
                    }
                    if let Some(v) = summaries.get(&id).filter(|_| {
                        self.math.config.summary_max_range_m <= 0.
                            || o.d_rel <= self.math.config.summary_max_range_m
                    }) {
                        extra.push((v + ego, self.math.config.summary_sigma_mps));
                    }
                    self.math.speed(id, time, &o, &extra, ego)?
                } else {
                    (o.v_long_ground * self.math.config.vground_scale, 0.)
                };
            let relative = ego.map_or(f64::NAN, |ego| ground - ego);
            let measured = if self.math.config.acc_range && acc == Some(id) {
                self._acc_pos.map_or(o.d_rel, |p| p.1)
            } else {
                o.d_rel
            };
            let distance = if self.math.config.range_fusion_gain > 0. {
                self.math.range(id, time, measured, relative)
            } else {
                measured
            };
            if o.age >= 2 {
                self._first
                    .entry(id)
                    .or_insert_with(|| vec![time, o.d_rel, o.y_rel]);
                self._last
                    .insert(id, vec![time, o.d_rel, o.y_rel, relative, o.age as f64]);
            }
            let c = &self.math.config;
            if o.age < c.min_publish_age {
                self.settling_suppressed += 1;
                continue;
            }
            if c.far_acc_only_m > 0. && acc != Some(id) && distance > c.far_acc_only_m {
                self.path_gated += 1;
                continue;
            }
            let still = c.path_gate_stationary_mps > 0.
                && ground.abs() < c.path_gate_stationary_mps
                && distance <= c.path_gate_min_range_m;
            if let Some(ego) = ego.filter(|_| {
                c.path_gate_m > 0.
                    && acc != Some(id)
                    && (distance > c.path_gate_min_range_m || still)
            }) {
                let (width, curve) = if still {
                    (c.path_gate_stationary_m, maximum(ego, 2.))
                } else {
                    (c.path_gate_m, maximum(ego, 5.))
                };
                if self.yaw(time).is_some_and(|yaw| {
                    (o.y_rel - yaw / curve * distance * distance / 2.).abs() > width
                }) {
                    self.path_gated += 1;
                    continue;
                }
            }
            if !self._out_id.contains_key(&id)
                && std > c.publish_speed_std_mps
                && c.fused_speed_filter
            {
                self.settling_suppressed += 1;
                continue;
            }
            if c.drop_unresolved_vrel && !relative.is_finite() {
                self.unresolved_suppressed += 1;
                continue;
            }
            if !self._out_id.contains_key(&id) {
                let out = self.relink(id, time, &seen);
                self._out_id.insert(id, out);
            }
            let yv = if [0, 1023].contains(&o.v_lat_code) {
                f64::NAN
            } else {
                self.yaw(time)
                    .map_or(f64::NAN, |yaw| o.v_lat_ground - yaw * o.d_rel)
            };
            let acceleration = if ego.is_none() || [-511, 512].contains(&o.accel_like_code) {
                f64::NAN
            } else {
                self._a_ego
                    .map_or(f64::NAN, |a| o.accel_like_code as f64 * 0.04 - a)
            };
            points.push(Point {
                track_id: self._out_id[&id]
                    + 10_000_000 * self._guard_gen.get(&id).copied().unwrap_or(0),
                d_rel: Number(distance),
                y_rel: Number(o.y_rel),
                v_rel: Number(relative),
                a_rel: Number(acceleration),
                yv_rel: Number(yv),
                measured: !o.predicted,
                metadata: self.math.config.include_metadata.then_some(Metadata {
                    slot,
                    age: o.age,
                    v_long_ground: Number(ground),
                    native_id: id,
                    existence_pct: o.existence_pct,
                    move_state: o.move_state,
                    oncoming_flag: o.oncoming_flag,
                    vrel_status: if ego.is_some() {
                        "native_over_ground_minus_ego"
                    } else {
                        "unresolved_nan"
                    },
                }),
            });
        }
        self.prune(time);
        Ok(Output {
            time_s: Number(time),
            radar_data: RadarData {
                points,
                errors: [
                    ("canError", false),
                    ("radarFault", false),
                    ("wrongConfig", false),
                    ("radarUnavailableTemporary", false),
                ]
                .into(),
            },
        })
    }
    fn prune(&mut self, time: f64) {
        if self._last.len() > 400 {
            let horizon = time - maximum(self.math.config.relink_max_gap_s, 1.) - 60.;
            self._first
                .retain(|id, _| !self._last.get(id).is_some_and(|v| v[0] < horizon));
            self._last
                .retain(|_, v| v[0].partial_cmp(&horizon) != Some(Ordering::Less));
        }
        for store in [&mut self.math.range_est, &mut self.math.fused] {
            if store.len() > 400 {
                store.retain(|_, v| v[0].partial_cmp(&(time - 5.)) != Some(Ordering::Less));
            }
        }
        if self._guard_state.len() > 400 {
            self._guard_state
                .retain(|_, v| v.0.partial_cmp(&(time - 5.)) != Some(Ordering::Less));
        }
        if self
            ._out_id
            .len()
            .max(self._claimed.len())
            .max(self._guard_gen.len())
            > 400
        {
            let retained: BTreeSet<_> = self
                ._last
                .keys()
                .copied()
                .chain(self._tracks.ids())
                .collect();
            self._out_id.retain(|id, _| retained.contains(id));
            self._guard_gen.retain(|id, _| retained.contains(id));
            self._claimed.retain(|id| retained.contains(id));
        }
    }
}
