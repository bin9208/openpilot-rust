//! Current-model edge exclusion at scan and final native publication coordinates.
use super::{
    birth_b5::{Model, Number},
    physical::Object,
    unsupported_birth::{Frame, Observation, UnsupportedBirth},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
pub type Line = (Vec<Number>, Vec<Number>);
pub type Sample = (Vec<Line>, Vec<Number>);
pub type GeometryKey = Vec<(Vec<u64>, Vec<u64>)>;
#[derive(Clone)]
pub struct Validated {
    pub key: GeometryKey,
    pub lines: Vec<Line>,
}
#[derive(Clone)]
pub struct Ingest {
    pub model: Option<Arc<Model>>,
    pub timestamp: i64,
    pub invalid: bool,
}
#[derive(Clone)]
pub struct RoadEdge {
    pub enabled: bool,
    pub model_edges: Vec<(i64, Option<Sample>)>,
    pub publication_model_edges: Vec<(i64, Option<Sample>)>,
    pub last_ns: Option<i64>,
    pub source_model_ns: Option<i64>,
    pub would_suppress: BTreeSet<u32>,
    pub publication_suppressed: usize,
    pub validated: Option<Validated>,
    pub last_ingest: Option<Ingest>,
}
pub struct ScanObject {
    pub id: u32,
    pub timestamp: i64,
    pub d: f64,
    pub y: f64,
    pub oem: bool,
    pub vision: bool,
}
impl From<&Object> for ScanObject {
    fn from(o: &Object) -> Self {
        Self {
            id: o.physical_track_id,
            timestamp: o.timestamp_ns,
            d: o.d_rel,
            y: o.y_rel,
            oem: o.oem_selected,
            vision: o.vision_supported,
        }
    }
}
impl RoadEdge {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            model_edges: vec![],
            publication_model_edges: vec![],
            last_ns: None,
            source_model_ns: None,
            would_suppress: BTreeSet::new(),
            publication_suppressed: 0,
            validated: None,
            last_ingest: None,
        }
    }
    pub fn invalidate_model(&mut self) {
        self.model_edges.clear();
        self.publication_model_edges.clear();
        self.validated = None;
        self.last_ingest = None;
        self.source_model_ns = None;
        self.would_suppress.clear();
    }
    pub fn reset(&mut self) {
        self.invalidate_model();
        self.last_ns = None;
    }
    pub fn ingest_model(&mut self, model: Option<Arc<Model>>, timestamp: i64) {
        if let (Some(last), Some(current)) = (&self.last_ingest, &model) {
            if last.timestamp == timestamp
                && last.model.as_ref().is_some_and(|m| Arc::ptr_eq(m, current))
            {
                if last.invalid {
                    self.would_suppress.clear()
                }
                return;
            }
        }
        let mut publication = None;
        if let Some(model) = &model {
            if model.edges.len() == 2
                && model.edge_stds.len() == 2
                && model.edge_stds.iter().all(|v| v.0.is_finite() && v.0 >= 0.)
            {
                let lines: Vec<_> = model
                    .edges
                    .iter()
                    .map(|line| (line.x.clone(), line.y.clone()))
                    .collect();
                let key: GeometryKey = lines
                    .iter()
                    .map(|(xs, ys)| {
                        (
                            xs.iter().map(|v| v.0.to_bits()).collect(),
                            ys.iter().map(|v| v.0.to_bits()).collect(),
                        )
                    })
                    .collect();
                if let Some(validated) = self.validated.as_ref().filter(|v| v.key == key) {
                    publication = Some((validated.lines.clone(), model.edge_stds.clone()));
                } else if lines.iter().all(|(xs, ys)| {
                    xs.len() >= 2
                        && xs.len() == ys.len()
                        && xs.iter().chain(ys).all(|v| v.0.is_finite())
                        && xs.windows(2).all(|p| p[0].0 < p[1].0)
                }) {
                    self.validated = Some(Validated {
                        key,
                        lines: lines.clone(),
                    });
                    publication = Some((lines, model.edge_stds.clone()));
                }
            }
        }
        let sample = publication
            .as_ref()
            .filter(|(_, stds)| stds.iter().all(|v| v.0 <= 1.))
            .cloned();
        insert_sample(&mut self.publication_model_edges, timestamp, publication);
        insert_sample(&mut self.model_edges, timestamp, sample.clone());
        let invalid = sample.is_none();
        if invalid {
            self.would_suppress.clear();
        }
        self.last_ingest = Some(Ingest {
            model,
            timestamp,
            invalid,
        });
    }
    pub fn update(
        &mut self,
        objects: &[ScanObject],
        timestamp: i64,
        valid: bool,
        camera: &BTreeMap<u32, (i32, i64)>,
        word0: &BTreeSet<u32>,
    ) {
        self.would_suppress.clear();
        self.source_model_ns = None;
        if self.last_ns.is_some_and(|last| timestamp < last) {
            self.reset()
        }
        self.last_ns = Some(timestamp);
        if !self.enabled || !valid {
            return;
        }
        let Some((ns, Some((lines, stds)))) =
            self.model_edges.iter().rev().find(|v| v.0 <= timestamp)
        else {
            return;
        };
        if !(0..=200_000_000).contains(&(timestamp - ns)) {
            return;
        }
        self.source_model_ns = Some(*ns);
        for o in objects {
            if o.timestamp != timestamp
                || o.oem
                || o.vision
                || word0.contains(&o.id)
                || camera.get(&o.id).is_some_and(|a| a.0 == 1 && a.1 >= 0)
            {
                continue;
            }
            let d = o.d + 1.52;
            let right = -o.y;
            if !d.is_finite()
                || !right.is_finite()
                || !lines
                    .iter()
                    .all(|(xs, _)| xs[0].0 <= d && d <= xs.last().unwrap().0)
            {
                continue;
            }
            let left = interp(&lines[0], d);
            let edge_right = interp(&lines[1], d);
            if left >= edge_right {
                continue;
            }
            if right < left - 1. - 2. * stds[0].0 || right > edge_right + 1. + 2. * stds[1].0 {
                self.would_suppress.insert(o.id);
            }
        }
    }
    pub fn publication_by<T: Clone>(
        &mut self,
        objects: &[T],
        timestamp: Option<i64>,
        fields: impl Fn(&T) -> (u32, i64),
    ) -> Vec<T> {
        let fresh = timestamp
            .zip(self.last_ns)
            .zip(self.source_model_ns)
            .is_some_and(|((t, last), source)| {
                (0..=200_000_000).contains(&(t - last)) && (0..=200_000_000).contains(&(t - source))
            });
        if !self.enabled || self.would_suppress.is_empty() || !fresh {
            return objects.to_vec();
        }
        let result: Vec<_> = objects
            .iter()
            .filter(|o| {
                let (id, ns) = fields(o);
                Some(ns) != self.last_ns || !self.would_suppress.contains(&id)
            })
            .cloned()
            .collect();
        self.publication_suppressed += objects.len() - result.len();
        result
    }
    pub fn publication_view(&mut self, objects: &[Object], timestamp: Option<i64>) -> Vec<Object> {
        self.publication_by(objects, timestamp, |o| {
            (o.physical_track_id, o.timestamp_ns)
        })
    }
    pub fn publication_surface_view(
        &mut self,
        frame: &Frame<'_>,
        unsupported: &UnsupportedBirth,
        objects: &[Object],
        timestamp: Option<i64>,
    ) -> Result<Vec<Object>, &'static str> {
        let Some(timestamp) = timestamp.filter(|_| {
            self.enabled && !objects.is_empty() && !frame.can_error && !frame.wrong_config
        }) else {
            return Ok(objects.to_vec());
        };
        let Some((ns, Some((lines, stds)))) = self
            .publication_model_edges
            .iter()
            .rev()
            .find(|v| v.0 <= timestamp)
        else {
            return Ok(objects.to_vec());
        };
        if !(0..=200_000_000).contains(&(timestamp - ns)) || !stds.iter().any(|v| v.0 <= 1.) {
            return Ok(objects.to_vec());
        }
        let mut kept = vec![];
        let mut curve_path = None;
        for obj in objects {
            if obj.members.len() != 1 || Some(obj.members[0].detection.timestamp_ns) != frame.scan {
                kept.push(obj.clone());
                continue;
            }
            let (wire_d, wire_y, wire_v) = surface_coordinates(obj, timestamp)?;
            let d = wire_d + 1.52;
            let right = -wire_y;
            if !d.is_finite()
                || !right.is_finite()
                || !wire_v.is_finite()
                || wire_d < 100.
                || !lines
                    .iter()
                    .all(|(xs, _)| xs[0].0 <= d && d <= xs.last().unwrap().0)
            {
                kept.push(obj.clone());
                continue;
            }
            let left = interp(&lines[0], d);
            let edge_right = interp(&lines[1], d);
            let outside = [
                stds[0].0 <= 1. && left - right - 1. - 2. * stds[0].0 > 0.,
                stds[1].0 <= 1. && right - edge_right - 1. - 2. * stds[1].0 > 0.,
            ];
            if left >= edge_right || !outside.iter().any(|v| *v) {
                kept.push(obj.clone());
                continue;
            }
            let observation = Observation::from(obj);
            let mut decision =
                unsupported.verdict(frame, &observation, Some(timestamp), false, true)?;
            if decision.exclude && stds.iter().all(|v| v.0 <= 1.) {
                continue;
            }
            if !decision.exclude && decision.reason == "AMBIGUOUS_RAW" {
                decision =
                    unsupported.verdict(frame, &observation, Some(timestamp), false, false)?;
            }
            if !decision.exclude {
                kept.push(obj.clone());
                continue;
            }
            let context = frame
                .context
                .expect("exclude verdict requires receive context");
            let path = &context.3;
            let valid =
                curve_path.get_or_insert_with(|| path.windows(2).all(|v| v[0].0 .0 < v[1].0 .0));
            if !*valid {
                kept.push(obj.clone());
                continue;
            }
            let heading = (path[1].1 .0 - path[0].1 .0) / (path[1].0 .0 - path[0].0 .0);
            let pathline = (
                path.iter().map(|p| p.0).collect(),
                path.iter().map(|p| p.1).collect(),
            );
            let bend =
                interp(&pathline, obj.d_rel) - path[0].1 .0 - heading * (obj.d_rel - path[0].0 .0);
            let yaw = context.2.unwrap().0;
            let exterior = if yaw < 0. && bend > 0. {
                Some(0)
            } else if yaw > 0. && bend < 0. {
                Some(1)
            } else {
                None
            };
            if exterior.is_none_or(|i| bend.abs() <= edge_right - left || !outside[i]) {
                kept.push(obj.clone());
            }
        }
        self.publication_suppressed += objects.len() - kept.len();
        Ok(kept)
    }
}
fn insert_sample(samples: &mut Vec<(i64, Option<Sample>)>, timestamp: i64, value: Option<Sample>) {
    samples.retain(|v| v.0 != timestamp);
    samples.push((timestamp, value));
    samples.sort_by_key(|v| v.0);
    if samples.len() > 4 {
        samples.drain(..samples.len() - 4);
    }
}
fn interp(line: &Line, x: f64) -> f64 {
    super::mirror_m3::interp(&line.0, &line.1, x)
}
pub fn surface_coordinates(obj: &Object, timestamp: i64) -> Result<(f64, f64, f64), &'static str> {
    let (_, d, y, v) = obj.published();
    let member = if obj.members.len() == 1 {
        &obj.members[0]
    } else {
        let id = obj.published().0;
        obj.members
            .iter()
            .find(|m| m.raw_track_id == id)
            .ok_or("")?
    };
    let (d, y, v) = (float32(d)?, float32(y)?, float32(v)?);
    Ok((
        float32(d + v * ((timestamp - member.detection.timestamp_ns) as f64 * 1e-9))?,
        y,
        v,
    ))
}
fn float32(v: f64) -> Result<f64, &'static str> {
    let f = v as f32;
    if f.is_infinite() && v.is_finite() {
        Err("float too large to pack with f format")
    } else {
        Ok(f as f64)
    }
}
