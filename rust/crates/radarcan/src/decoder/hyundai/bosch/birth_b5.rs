//! Frozen B5 birth proof, causal model alignment and one-publication deferral.
use super::{physical::Object, tracking::Track};
use crate::scalar::{float_sum, square};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Number(
    #[serde(
        serialize_with = "crate::scalar::serialize_float",
        deserialize_with = "crate::scalar::deserialize_float"
    )]
    pub f64,
);
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Line {
    #[serde(default)]
    pub x: Vec<Number>,
    #[serde(default)]
    pub y: Vec<Number>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Model {
    #[serde(rename = "timestampEof", default)]
    pub source_ns: i64,
    #[serde(rename = "roadEdges", default)]
    pub edges: Vec<Line>,
    #[serde(rename = "laneLines", default)]
    pub lanes: Vec<Line>,
    #[serde(default)]
    pub position: Option<Line>,
    #[serde(rename = "roadEdgeStds", default)]
    pub edge_stds: Vec<Number>,
    #[serde(rename = "laneLineProbs", default)]
    pub lane_probs: Vec<Number>,
    #[serde(rename = "laneLineStds", default)]
    pub lane_stds: Vec<Number>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Context {
    pub publication_ns: i64,
    pub source_ns: i64,
    pub road_edges: Vec<Vec<(f64, f64)>>,
    pub road_edge_stds: Vec<Number>,
    pub lane_lines: Vec<Vec<(f64, f64)>>,
    pub lane_probs: Vec<Number>,
    pub lane_stds: Vec<Number>,
    pub path: Vec<(f64, f64)>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Parent {
    pub raw_member_set: Vec<u32>,
    pub representative_raw_id: u32,
    pub stable_scans: usize,
    pub last_seen_ns: i64,
    pub lifecycle_birth_ns: i64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct BirthB5 {
    pub mode: i32,
    pub states: IndexMap<u32, Parent>,
    pub raw_prior_owners: BTreeMap<u32, BTreeSet<u32>>,
    pub speed_samples: Vec<(i64, Number)>,
    pub pose_samples: Vec<(i64, Number, bool)>,
    pub model_contexts: Vec<Context>,
    pub last_scan_ns: Option<i64>,
    pub last_decisions: Vec<Value>,
    pub would_suppress: BTreeSet<u32>,
    pub last_suppressed: Vec<u32>,
    #[serde(rename = "_counted_suppression_ns")]
    pub counted_suppression_ns: Option<i64>,
    pub suppressed_points: usize,
    pub suppressed_scans: usize,
    pub max_state_count: usize,
    pub max_raw_owner_count: usize,
    pub reset_count: usize,
    pub last_reset_reason: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Geometry {
    pub edges: Vec<Vec<(f64, f64)>>,
    pub lanes: Vec<Vec<(f64, f64)>>,
    pub path: Vec<(f64, f64)>,
    pub edge_stds: Vec<Number>,
    pub lane_probs: Vec<Number>,
    pub lane_stds: Vec<Number>,
    pub source_ns: i64,
}
#[derive(Clone, Serialize)]
pub struct Alignment {
    pub outside: f64,
    pub left: f64,
    pub right: f64,
    pub width: f64,
    pub path_inside: bool,
    pub edge_std: Number,
    pub edge_y: f64,
}
#[derive(Clone, Serialize)]
pub struct Instant {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lane_y: Option<[f64; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reliable_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<bool>,
}
impl BirthB5 {
    pub fn new(mode: i32) -> Self {
        Self {
            mode,
            states: IndexMap::new(),
            raw_prior_owners: BTreeMap::new(),
            speed_samples: vec![],
            pose_samples: vec![],
            model_contexts: vec![],
            last_scan_ns: None,
            last_decisions: vec![],
            would_suppress: BTreeSet::new(),
            last_suppressed: vec![],
            counted_suppression_ns: None,
            suppressed_points: 0,
            suppressed_scans: 0,
            max_state_count: 0,
            max_raw_owner_count: 0,
            reset_count: 0,
            last_reset_reason: "CONSTRUCTION".into(),
        }
    }
    pub fn reset(&mut self, reason: &str) {
        self.states.clear();
        self.raw_prior_owners.clear();
        self.speed_samples.clear();
        self.pose_samples.clear();
        self.model_contexts.clear();
        self.last_scan_ns = None;
        self.last_decisions.clear();
        self.would_suppress.clear();
        self.last_suppressed.clear();
        self.counted_suppression_ns = None;
        self.reset_count += 1;
        self.last_reset_reason = reason.into();
    }
    pub fn ingest_pose(&mut self, timestamp: i64, yaw: Option<f64>) {
        if timestamp <= 0 {
            return;
        }
        let valid = yaw.is_some_and(f64::is_finite);
        let sample = (
            timestamp,
            Number(yaw.filter(|v| *v != 0.).unwrap_or(0.)),
            valid,
        );
        append_sample(&mut self.pose_samples, sample, |s| s.0);
    }
    pub fn ingest_speed(&mut self, timestamp: i64, speed: f64) {
        if timestamp > 0 && speed.is_finite() {
            append_sample(&mut self.speed_samples, (timestamp, Number(speed)), |s| s.0);
        }
    }
    pub fn ingest_model(&mut self, model: Option<&Model>, publication: i64) {
        let Some(model) = model.filter(|_| publication > 0) else {
            return;
        };
        if self
            .model_contexts
            .last()
            .is_some_and(|c| c.publication_ns == publication && c.source_ns == model.source_ns)
        {
            return;
        }
        let Some(position) = &model.position else {
            return;
        };
        if model.source_ns <= 0
            || model.edges.len() != 2
            || model.lanes.len() != 4
            || model.edge_stds.len() != 2
            || model.lane_probs.len() != 4
            || model.lane_stds.len() != 4
        {
            return;
        }
        let context = Context {
            publication_ns: publication,
            source_ns: model.source_ns,
            road_edges: model.edges.iter().map(points).collect(),
            road_edge_stds: model.edge_stds.clone(),
            lane_lines: model.lanes.iter().map(points).collect(),
            lane_probs: model.lane_probs.clone(),
            lane_stds: model.lane_stds.clone(),
            path: points(position),
        };
        if context
            .road_edges
            .iter()
            .chain(&context.lane_lines)
            .any(|line| line.len() < 2)
            || context.path.len() < 2
        {
            return;
        }
        if self
            .model_contexts
            .last()
            .is_some_and(|c| publication < c.publication_ns)
        {
            self.model_contexts.clear();
        }
        if self
            .model_contexts
            .last()
            .is_some_and(|c| publication == c.publication_ns)
        {
            *self.model_contexts.last_mut().unwrap() = context;
        } else {
            self.model_contexts.push(context);
        }
        if self.model_contexts.len() > 4 {
            self.model_contexts.drain(..self.model_contexts.len() - 4);
        }
    }
    pub fn integrate_motion(&self, start: i64, end: i64) -> Option<(f64, f64, f64)> {
        let speeds = &self.speed_samples;
        let poses = &self.pose_samples;
        if end < start || speeds.first()?.0 > start || poses.first()?.0 > start {
            return None;
        }
        let (mut x, mut y, mut heading) = (0., 0., 0.);
        let mut timestamp = start;
        while timestamp < end {
            let next = (timestamp + 10_000_000).min(end);
            let middle = (timestamp + next) / 2;
            let pose_index = poses.partition_point(|s| s.0 <= middle).checked_sub(1)?;
            if middle - poses[pose_index].0 > 200_000_000 || !poses[pose_index].2 {
                return None;
            }
            let speed = interpolate(speeds, middle, |s| (s.0, s.1 .0));
            let yaw = interpolate(poses, middle, |s| (s.0, s.1 .0));
            if !speed.is_finite() || !yaw.is_finite() {
                return None;
            }
            let dt = (next - timestamp) as f64 * 1e-9;
            let middle_heading = heading + 0.5 * yaw * dt;
            x += speed * middle_heading.cos() * dt;
            y += speed * middle_heading.sin() * dt;
            heading += yaw * dt;
            timestamp = next;
        }
        Some((x, y, heading))
    }
    pub fn geometry(&self, context: &Context, scan: i64) -> Option<Geometry> {
        if context.source_ns > scan || scan - context.source_ns > 200_000_000 {
            return None;
        }
        let (dx, dy, heading) = self.integrate_motion(context.source_ns, scan)?;
        let trig = (heading.cos(), heading.sin());
        Some(Geometry {
            edges: context
                .road_edges
                .iter()
                .map(|p| transform(p, dx, dy, trig))
                .collect(),
            lanes: context
                .lane_lines
                .iter()
                .map(|p| transform(p, dx, dy, trig))
                .collect(),
            path: transform(&context.path, dx, dy, trig),
            edge_stds: context.road_edge_stds.clone(),
            lane_probs: context.lane_probs.clone(),
            lane_stds: context.lane_stds.clone(),
            source_ns: context.source_ns,
        })
    }
    fn latest_context(&self, scan: i64, now: i64) -> Option<usize> {
        let index = self
            .model_contexts
            .iter()
            .rposition(|c| c.publication_ns <= now)?;
        let c = &self.model_contexts[index];
        if now - c.publication_ns > 200_000_000
            || c.source_ns > scan
            || scan - c.source_ns > 200_000_000
        {
            None
        } else {
            Some(index)
        }
    }
    pub fn align(obj: &Object, geometry: &Geometry) -> Option<Alignment> {
        let left = interp(&geometry.edges[0], obj.d_rel);
        let right = interp(&geometry.edges[1], obj.d_rel);
        if !left.is_finite() || !right.is_finite() || left <= right {
            return None;
        }
        let path = interp(&geometry.path, obj.d_rel);
        let is_left = obj.y_rel >= 0.;
        Some(Alignment {
            outside: if is_left {
                obj.y_rel - left
            } else {
                right - obj.y_rel
            },
            left,
            right,
            width: left - right,
            path_inside: path.is_finite() && right <= path && path <= left,
            edge_std: geometry.edge_stds[usize::from(!is_left)],
            edge_y: if is_left { left } else { right },
        })
    }
}
fn append_sample<T: PartialEq>(samples: &mut Vec<T>, sample: T, time: impl Fn(&T) -> i64) {
    let timestamp = time(&sample);
    if samples.last() == Some(&sample) {
        return;
    }
    if samples.last().is_some_and(|s| timestamp < time(s)) {
        samples.clear();
    }
    if samples.last().is_some_and(|s| timestamp == time(s)) {
        *samples.last_mut().unwrap() = sample;
    } else {
        samples.push(sample);
    }
    let first = samples.partition_point(|s| time(s) < timestamp - 400_000_000);
    samples.drain(..first);
    if samples.len() > 64 {
        samples.drain(..samples.len() - 64);
    }
}
fn points(line: &Line) -> Vec<(f64, f64)> {
    line.x
        .iter()
        .zip(&line.y)
        .filter(|(x, y)| x.0.is_finite() && y.0.is_finite())
        .map(|(x, y)| (x.0, y.0))
        .collect()
}
fn interpolate<T>(samples: &[T], timestamp: i64, row: impl Fn(&T) -> (i64, f64)) -> f64 {
    let index = samples.partition_point(|s| row(s).0 < timestamp);
    if index == 0 {
        return row(&samples[0]).1;
    }
    if index >= samples.len() {
        return row(samples.last().unwrap()).1;
    }
    let a = row(&samples[index - 1]);
    let b = row(&samples[index]);
    if b.0 == a.0 {
        a.1
    } else {
        a.1 + (timestamp - a.0) as f64 / (b.0 - a.0) as f64 * (b.1 - a.1)
    }
}
fn transform(
    points: &[(f64, f64)],
    dx: f64,
    dy: f64,
    (cosine, sine): (f64, f64),
) -> Vec<(f64, f64)> {
    let neg = -sine;
    let mut out: Vec<_> = points
        .iter()
        .map(|&(mx, my)| {
            let x = mx - 1.52 - dx;
            let y = -my - dy;
            (cosine * x + sine * y, neg * x + cosine * y)
        })
        .collect();
    out.sort_by(|a, b| cmp(a.0, b.0).then_with(|| cmp(a.1, b.1)));
    out
}
fn cmp(a: f64, b: f64) -> std::cmp::Ordering {
    a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
}
fn interp(points: &[(f64, f64)], x: f64) -> f64 {
    if points.len() < 2 || x < points[0].0 || x > points.last().unwrap().0 {
        return f64::NAN;
    }
    let i = points.partition_point(|p| p.0 < x);
    if i == 0 {
        return points[0].1;
    }
    if i >= points.len() {
        return points.last().unwrap().1;
    }
    let (x0, y0) = points[i - 1];
    let (x1, y1) = points[i];
    if x1 == x0 {
        0.5 * (y0 + y1)
    } else {
        y0 + (x - x0) / (x1 - x0) * (y1 - y0)
    }
}
#[derive(Default)]
struct ScanCache {
    geometries: BTreeMap<usize, Option<Geometry>>,
    aligned: Option<BTreeMap<u32, Option<Alignment>>>,
}
pub struct Frame<'a> {
    pub objects: &'a [Object],
    pub raw: &'a [Track],
    pub scan: i64,
    pub now: i64,
    pub ego: f64,
    pub yaw: Option<f64>,
    pub camera: &'a BTreeMap<u32, (i32, i64)>,
    pub oem_state: u8,
    pub word0: &'a BTreeSet<u32>,
    pub live_pids: &'a BTreeSet<u32>,
    pub live_raw: &'a BTreeSet<u32>,
}
impl BirthB5 {
    pub fn instant_lane(obj: &Object, g: &Geometry) -> Instant {
        let unavailable = || Instant {
            available: false,
            lane_y: None,
            reliable_count: None,
            evidence: None,
        };
        let left = interp(&g.edges[0], obj.d_rel);
        let right = interp(&g.edges[1], obj.d_rel);
        let values: Vec<_> = g.lanes.iter().map(|p| interp(p, obj.d_rel)).collect();
        if !left.is_finite()
            || !right.is_finite()
            || left <= right
            || values.len() != 4
            || values.iter().any(|v| !v.is_finite())
        {
            return unavailable();
        }
        let lane_y: [f64; 4] = values.try_into().unwrap();
        let reliable: [bool; 4] =
            std::array::from_fn(|i| g.lane_probs[i].0 >= 0.5 && g.lane_stds[i].0 <= 0.5);
        let sign = if obj.y_rel >= 0. { 1. } else { -1. };
        let edge = if sign > 0. { left } else { right };
        let mut entries: Vec<_> = lane_y.iter().enumerate().map(|(i, y)| (*y, i)).collect();
        entries.sort_by(|a, b| cmp(b.0, a.0).then_with(|| b.1.cmp(&a.1)));
        let pair = entries.windows(2).any(|p| {
            let (high, hi) = p[0];
            let (low, lo) = p[1];
            reliable[hi]
                && reliable[lo]
                && low <= obj.y_rel
                && obj.y_rel <= high
                && (2.0..=5.5).contains(&(high - low))
                && sign * (0.5 * (high + low) - edge) >= 0.
        });
        let band = lane_y.iter().enumerate().any(|(i, value)| {
            reliable[i]
                && (2.0..=7.0).contains(&(sign * (value - edge)))
                && sign * (value - obj.y_rel) >= 0.
        });
        let inside: Vec<_> = lane_y
            .iter()
            .enumerate()
            .filter(|(i, value)| reliable[*i] && sign * (edge - **value) >= 0.)
            .map(|(i, value)| (sign * (edge - value), i))
            .collect();
        let mut diverging = false;
        if let Some(&(gap, index)) = inside
            .iter()
            .min_by(|a, b| cmp(a.0, b.0).then_with(|| a.1.cmp(&b.1)))
        {
            let lane = &g.lanes[index];
            let edge = &g.edges[usize::from(sign <= 0.)];
            let minimum = lane[0].0.max(edge[0].0);
            let anchor = minimum.max(obj.d_rel - 20.);
            if obj.d_rel - anchor >= 10. {
                let ly = interp(lane, anchor);
                let ey = interp(edge, anchor);
                if ly.is_finite() && ey.is_finite() {
                    let anchor_gap = sign * (ey - ly);
                    diverging = gap >= 1. && gap - anchor_gap >= 0.75;
                }
            }
        }
        Instant {
            available: true,
            lane_y: Some(lane_y),
            reliable_count: Some(reliable.into_iter().filter(|v| *v).count()),
            evidence: Some(pair || band || diverging),
        }
    }
    pub fn corridor_clear(&self, obj: &Object, scan: i64, now: i64) -> Result<bool, &'static str> {
        self.corridor_cached(obj, scan, now, &mut ScanCache::default())
    }
    fn corridor_cached(
        &self,
        obj: &Object,
        scan: i64,
        now: i64,
        cache: &mut ScanCache,
    ) -> Result<bool, &'static str> {
        if self.latest_context(scan, now).is_none() {
            return Ok(false);
        }
        let mut samples = vec![];
        for (index, context) in self.model_contexts.iter().enumerate() {
            if context.publication_ns > now
                || context.source_ns > scan
                || context.source_ns < scan - 120_000_000
            {
                continue;
            }
            let geometry = cache
                .geometries
                .entry(index)
                .or_insert_with(|| self.geometry(context, scan));
            let Some(geometry) = geometry.as_ref() else {
                continue;
            };
            let instant = Self::instant_lane(obj, geometry);
            samples.push((context.source_ns, instant));
        }
        samples.sort_by_key(|(ns, _)| *ns);
        let valid: Vec<_> = samples.iter().filter(|(_, i)| i.available).collect();
        if valid.len() < 2 || valid.last().unwrap().0 - valid[0].0 < 40_000_000 {
            return Ok(false);
        }
        let mut stds = vec![];
        for index in 0..4 {
            let values: Vec<_> = valid
                .iter()
                .map(|(_, i)| i.lane_y.unwrap()[index])
                .collect();
            let std = population_std(&values)?;
            if std.is_finite() {
                stds.push(std);
            }
        }
        if stds.is_empty()
            || stds.into_iter().reduce(f64::max).unwrap() > 0.5
            || valid[valid.len() - 2..]
                .iter()
                .map(|(_, i)| i.reliable_count.unwrap())
                .min()
                .unwrap()
                < 2
        {
            return Ok(false);
        }
        Ok(!valid[valid.len() - 2..]
            .iter()
            .all(|(_, i)| i.evidence.unwrap()))
    }
    fn n4_parent<'a>(
        &self,
        target: &Object,
        f: &Frame<'a>,
        prior_raw: &BTreeMap<u32, BTreeSet<u32>>,
        cache: &mut ScanCache,
    ) -> Result<(Option<&'a Object>, &'static str), &'static str> {
        if !f.ego.is_finite() || f.ego < 3. {
            return Ok((None, "LOW_SPEED_FAIL_OPEN"));
        }
        let Some(yaw) = f.yaw.filter(|v| v.is_finite()) else {
            return Ok((None, "POSE_UNAVAILABLE_FAIL_OPEN"));
        };
        if (yaw / f.ego.max(0.1)).abs() > 0.02 {
            return Ok((None, "HIGH_CURVATURE_FAIL_OPEN"));
        }
        let Some(index) = self.latest_context(f.scan, f.now) else {
            return Ok((None, "MODEL_STALE_FAIL_OPEN"));
        };
        let geometry = cache
            .geometries
            .entry(index)
            .or_insert_with(|| self.geometry(&self.model_contexts[index], f.scan));
        let Some(geometry) = geometry.as_ref() else {
            return Ok((None, "POSE_UNAVAILABLE_FAIL_OPEN"));
        };
        if cache.aligned.is_none() {
            cache.aligned = Some(
                f.objects
                    .iter()
                    .map(|o| (o.physical_track_id, Self::align(o, geometry)))
                    .collect(),
            );
        }
        let aligned = cache.aligned.as_ref().unwrap();
        let target_alignment = aligned.get(&target.physical_track_id).cloned().flatten();
        let Some(target_alignment) = target_alignment.filter(|a| {
            a.path_inside
                && (2.0..=20.0).contains(&a.width)
                && a.edge_std.0.is_finite()
                && a.edge_std.0 <= 0.5
        }) else {
            return Ok((None, "ROAD_EDGE_UNSTABLE_FAIL_OPEN"));
        };
        if target_alignment.outside < 1. {
            return Ok((None, "OUTSIDE_MARGIN_FAIL_OPEN"));
        }
        let target_world = target.v_rel + f.ego - yaw * target.y_rel;
        if target_world <= 1. {
            return Ok((None, "TARGET_DIRECTION_FAIL_OPEN"));
        }
        if !self.corridor_cached(target, f.scan, f.now, cache)? {
            return Ok((None, "CORRIDOR_UNKNOWN_OR_POSITIVE_FAIL_OPEN"));
        }
        let aligned = cache.aligned.as_ref().unwrap();
        let mut candidates = vec![];
        for parent in f.objects {
            if parent.physical_track_id == target.physical_track_id {
                continue;
            }
            let alignment = aligned
                .get(&parent.physical_track_id)
                .and_then(Option::as_ref);
            let world = parent.v_rel + f.ego - yaw * parent.y_rel;
            if alignment.is_none_or(|a| a.outside > 0.) || parent.age_scans < 20 || world <= 1. {
                continue;
            }
            if (target.d_rel - parent.d_rel).abs() <= 12.
                && (target.y_rel - parent.y_rel).abs() <= 10.
                && (target.v_rel - parent.v_rel).abs() <= 1.5
                && (target_world - world).abs() <= 1.5
            {
                let score = (target_world - world).abs()
                    + (target.d_rel - parent.d_rel).abs() / 12.
                    + (target.y_rel - parent.y_rel).abs() / 10.;
                candidates.push((score, parent));
            }
        }
        if candidates.len() != 1 {
            return Ok((
                None,
                if candidates.is_empty() {
                    "NO_PARENT_FAIL_OPEN"
                } else {
                    "AMBIGUOUS_PARENT_FAIL_OPEN"
                },
            ));
        }
        let parent = candidates[0].1;
        let target_assoc = f.camera.get(&target.physical_track_id);
        let parent_assoc = f.camera.get(&parent.physical_track_id);
        if target_assoc.is_some_and(|&(state, episode)| {
            state == 1 && episode >= 0 && parent_assoc.is_none_or(|&(p, e)| p != 1 || e != episode)
        }) {
            return Ok((None, "INDEPENDENT_CAMERA_FAIL_OPEN"));
        }
        if f.oem_state == 3 && f.word0.contains(&target.physical_track_id) {
            return Ok((None, "INDEPENDENT_OEM_FAIL_OPEN"));
        }
        if target.members.iter().any(|m| {
            prior_raw
                .get(&m.raw_track_id)
                .is_some_and(|owners| owners.iter().any(|pid| *pid != parent.physical_track_id))
        }) {
            return Ok((None, "INDEPENDENT_RAW_FAIL_OPEN"));
        }
        Ok((Some(parent), "N4_FROZEN"))
    }
    pub fn update(&mut self, f: Frame<'_>) -> Result<BTreeSet<u32>, &'static str> {
        if self.mode == 0 {
            self.last_decisions.clear();
            self.would_suppress.clear();
            return Ok(self.would_suppress.clone());
        }
        if let Some(last) = self.last_scan_ns {
            if f.scan <= last {
                self.reset("CLOCK_RESET");
            } else if f.scan - last >= 300_000_000 {
                self.reset("RADAR_INPUT_GAP");
            }
        }
        self.last_scan_ns = Some(f.scan);
        self.states.retain(|pid, _| f.live_pids.contains(pid));
        self.raw_prior_owners
            .retain(|raw, _| f.live_raw.contains(raw));
        let prior_raw = self.raw_prior_owners.clone();
        for obj in f.objects {
            let pid = obj.physical_track_id;
            let mut structure: Vec<_> = obj.members.iter().map(|m| m.raw_track_id).collect();
            structure.sort_unstable();
            let previous = self.states.get(&pid);
            if previous.is_none_or(|p| {
                obj.age_scans == 1
                    || p.raw_member_set != structure
                    || p.representative_raw_id != obj.representative_raw_track_id
            }) {
                let birth = previous
                    .filter(|_| obj.age_scans != 1)
                    .map_or(f.scan, |p| p.lifecycle_birth_ns);
                self.states.insert(
                    pid,
                    Parent {
                        raw_member_set: structure,
                        representative_raw_id: obj.representative_raw_track_id,
                        stable_scans: 1,
                        last_seen_ns: f.scan,
                        lifecycle_birth_ns: birth,
                    },
                );
            } else {
                let previous = self.states.get_mut(&pid).unwrap();
                previous.stable_scans = (previous.stable_scans + 1).min(255);
                previous.last_seen_ns = f.scan;
            }
        }
        self.max_state_count = self.max_state_count.max(self.states.len());
        let new_raw = f.raw.iter().filter(|r| r.age_scans == 1).count();
        let newborns: Vec<_> = f.objects.iter().filter(|o| o.age_scans == 1).collect();
        let new_pids = newborns.len();
        let mut decisions = vec![];
        let mut suppressed = BTreeSet::new();
        let mut cache = ScanCache::default();
        for target in newborns {
            let singleton = target.members.len() == 1;
            let (parent, mut reason) = self.n4_parent(target, &f, &prior_raw, &mut cache)?;
            let n4 = parent.is_some();
            let stable = parent
                .and_then(|p| self.states.get(&p.physical_track_id))
                .map_or(0, |p| p.stable_scans);
            let target_code = representative_code(target);
            let parent_code = parent.and_then(representative_code);
            let decision = if !n4 {
                false
            } else if !singleton {
                reason = "NON_SINGLETON_FAIL_OPEN";
                false
            } else if target_code.is_none() || parent_code.is_none() || target_code != parent_code {
                reason = "VREL_NOT_EXACT_FAIL_OPEN";
                false
            } else if new_raw != 1 {
                reason = "NEW_RAW_COUNT_FAIL_OPEN";
                false
            } else if new_pids != 1 {
                reason = "NEW_PID_COUNT_FAIL_OPEN";
                false
            } else if stable < 250 {
                reason = "PARENT_NOT_STABLE_FAIL_OPEN";
                false
            } else {
                reason = "BIRTH_B5_ACTIVE_DEFER";
                suppressed.insert(target.physical_track_id);
                true
            };
            decisions.push(json!({"timestamp_ns":f.scan,"target_pid":target.physical_track_id,"parent_pid":parent.map(|p|p.physical_track_id),"parent_stable_scans":stable,"target_vrel_code":target_code,"parent_vrel_code":parent_code,"new_raw_count":new_raw,"new_pid_count":new_pids,"singleton":singleton,"n4_eligible":n4,"decision":decision,"reason":reason}));
        }
        for obj in f.objects {
            for member in &obj.members {
                self.raw_prior_owners
                    .entry(member.raw_track_id)
                    .or_default()
                    .insert(obj.physical_track_id);
            }
        }
        self.max_raw_owner_count = self.max_raw_owner_count.max(self.raw_prior_owners.len());
        self.last_decisions = decisions;
        self.would_suppress = suppressed;
        self.last_suppressed.clear();
        Ok(self.would_suppress.clone())
    }
    pub fn publication_view(&mut self, objects: &[Object]) -> Vec<Object> {
        if self.mode != 1
            || objects.is_empty()
            || self.would_suppress.is_empty()
            || objects
                .iter()
                .any(|o| Some(o.timestamp_ns) != self.last_scan_ns)
        {
            self.last_suppressed.clear();
            return objects.to_vec();
        }
        let mut suppressed: Vec<_> = objects
            .iter()
            .filter(|o| self.would_suppress.contains(&o.physical_track_id))
            .map(|o| o.physical_track_id)
            .collect();
        suppressed.sort_unstable();
        self.last_suppressed = suppressed;
        if self.last_suppressed.is_empty() {
            return objects.to_vec();
        }
        if self.counted_suppression_ns != self.last_scan_ns {
            self.suppressed_points += self.last_suppressed.len();
            self.suppressed_scans += 1;
            self.counted_suppression_ns = self.last_scan_ns;
        }
        let hidden: BTreeSet<_> = self.last_suppressed.iter().copied().collect();
        objects
            .iter()
            .filter(|o| !hidden.contains(&o.physical_track_id))
            .cloned()
            .collect()
    }
}
fn population_std(values: &[f64]) -> Result<f64, &'static str> {
    if values.len() < 2 {
        return Ok(f64::NAN);
    }
    let mean = float_sum(values.iter().copied()) / values.len() as f64;
    let squares = values
        .iter()
        .map(|v| square(v - mean).map_err(|_| "(34, 'Numerical result out of range')"))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((float_sum(squares) / values.len() as f64).sqrt())
}
fn representative_code(obj: &Object) -> Option<u32> {
    obj.members
        .iter()
        .find(|m| m.raw_track_id == obj.representative_raw_track_id)
        .map(|m| (m.detection.raw_word >> 21) & 0x3ff)
}
