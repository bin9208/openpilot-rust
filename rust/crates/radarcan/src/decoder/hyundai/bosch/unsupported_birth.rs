//! Publication-only unsupported first-singleton verdict; tracking is untouched.
use super::{
    birth_b5::Number,
    camera::CameraObject,
    camera_association::{associate, Measurement},
    physical::Object,
    unsupported_path::birth_d_path,
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Serialize, Deserialize)]
pub struct Context(
    pub i64,
    pub Number,
    pub Option<Number>,
    pub Vec<(Number, Number)>,
    pub Option<i64>,
);
#[derive(Clone, Serialize, Deserialize)]
pub struct Observation {
    pub physical_track_id: u32,
    pub d_rel: Number,
    pub y_rel: Number,
    pub v_rel: Number,
    pub age_scans: usize,
    pub timestamp_ns: i64,
    pub oem_selected: bool,
    pub vision_supported: bool,
    pub members: Vec<(u32, i64)>,
}
impl From<&Object> for Observation {
    fn from(o: &Object) -> Self {
        Self {
            physical_track_id: o.physical_track_id,
            d_rel: Number(o.d_rel),
            y_rel: Number(o.y_rel),
            v_rel: Number(o.v_rel),
            age_scans: o.age_scans,
            timestamp_ns: o.timestamp_ns,
            oem_selected: o.oem_selected,
            vision_supported: o.vision_supported,
            members: o
                .members
                .iter()
                .map(|m| (m.raw_track_id, m.detection.timestamp_ns))
                .collect(),
        }
    }
}
#[derive(Clone, Copy)]
pub struct Snapshot<'a> {
    pub objects: &'a [CameraObject],
    pub count: usize,
    pub complete: i64,
}
pub struct Frame<'a> {
    pub can_error: bool,
    pub wrong_config: bool,
    pub context: Option<&'a Context>,
    pub scan: Option<i64>,
    pub identity: &'a BTreeMap<u32, (i64, usize, bool)>,
    pub extended_ns: Option<i64>,
    pub associations: &'a IndexMap<u32, (i32, i64, i64)>,
    pub processed: &'a BTreeSet<u32>,
    pub snapshot: Option<Snapshot<'a>>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Details {
    #[serde(rename = "dPath")]
    pub d_path: Number,
    pub world_longitudinal_speed_approx: Number,
    pub current_a0: (i32, i64, i64),
    pub source_ns: i64,
    pub snapshot_complete_ns: i64,
    pub publication_ns: i64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Decision {
    pub exclude: bool,
    pub reason: String,
    #[serde(flatten)]
    pub details: Option<Details>,
}
impl Decision {
    fn keep(reason: &str) -> Self {
        Self {
            exclude: false,
            reason: reason.into(),
            details: None,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct UnsupportedBirth {
    pub enabled: bool,
    pub last_decisions: IndexMap<u32, Decision>,
}
impl UnsupportedBirth {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            last_decisions: IndexMap::new(),
        }
    }
    fn context<'a>(
        frame: &'a Frame<'_>,
        publication: Option<i64>,
    ) -> Result<(&'a Context, i64), &'static str> {
        let (Some(context), Some(publication)) = (frame.context, publication) else {
            return Err("MISSING_RECEIVE_CONTEXT");
        };
        if !(0..=200_000_000).contains(&(publication - context.0)) {
            return Err("STALE_EGO_CONTEXT");
        }
        let ego = context.1 .0;
        let Some(yaw) = context.2.map(|v| v.0) else {
            return Err("MISSING_OR_INVALID_MOTION");
        };
        if !ego.is_finite() || ego < 3. || !yaw.is_finite() || (yaw / ego).abs() > 0.02 {
            return Err("MISSING_OR_INVALID_MOTION");
        }
        if context
            .4
            .is_none_or(|ns| !(0..=200_000_000).contains(&(publication - ns)))
            || context.3.len() < 2
            || context
                .3
                .iter()
                .any(|p| !p.0 .0.is_finite() || !p.1 .0.is_finite())
        {
            return Err("MISSING_OR_STALE_PATH");
        }
        let Some(scan) = frame.scan else {
            return Err("STALE_SCAN");
        };
        if !(scan <= context.0 && context.0 <= publication) || publication - scan > 150_000_000 {
            return Err("STALE_SCAN");
        }
        Ok((context, scan))
    }
    pub fn verdict(
        &self,
        frame: &Frame<'_>,
        obj: &Observation,
        publication: Option<i64>,
        first_only: bool,
        require_single_candidate: bool,
    ) -> Result<Decision, &'static str> {
        if !self.enabled {
            return Ok(Decision::keep("DISABLED"));
        }
        if frame.can_error || frame.wrong_config {
            return Ok(Decision::keep("INVALID_PROVIDER"));
        }
        if obj.members.len() != 1 || (first_only && obj.age_scans != 1) {
            return Ok(Decision::keep("NOT_FIRST_SINGLETON_OBSERVATION"));
        }
        let (context, scan) = match Self::context(frame, publication) {
            Ok(v) => v,
            Err(reason) => return Ok(Decision::keep(reason)),
        };
        let publication = publication.unwrap();
        if obj.timestamp_ns != scan {
            return Ok(Decision::keep("NOT_FIRST_SINGLETON_OBSERVATION"));
        }
        let member = obj.members[0];
        let Some(identity) = frame.identity.get(&member.0) else {
            return Ok(Decision::keep("MISSING_RAW_LINK"));
        };
        if identity.0 != member.1 || member.1 > scan {
            return Ok(Decision::keep("MISSING_RAW_LINK"));
        }
        if (require_single_candidate && identity.1 > 1) || identity.2 {
            return Ok(Decision::keep("AMBIGUOUS_RAW"));
        }
        let (d, y, v) = (obj.d_rel.0, obj.y_rel.0, obj.v_rel.0);
        if !d.is_finite() || !y.is_finite() || !v.is_finite() {
            return Ok(Decision::keep("INVALID_MEASUREMENT"));
        }
        let ego = context.1 .0;
        if (ego + v).abs() <= 1. {
            return Ok(Decision::keep("STATIONARY_LIKE"));
        }
        if d < context.3[0].0 .0 || d > context.3.last().unwrap().0 .0 {
            return Ok(Decision::keep("OUTSIDE_PATH_EXTENT"));
        }
        let path: Vec<_> = context.3.iter().map(|p| (p.0 .0, p.1 .0)).collect();
        let d_path = birth_d_path(&path, d, y)?;
        let Some(d_path) = d_path.filter(|v| v.is_finite() && v.abs() > 1.8) else {
            return Ok(Decision::keep("CURRENT_PATH_CORRIDOR"));
        };
        let prior = frame.associations.get(&obj.physical_track_id);
        if obj.vision_supported
            || obj.oem_selected
            || frame.processed.contains(&obj.physical_track_id)
            || (frame.extended_ns == Some(scan) && prior.is_some_and(|v| v.0 == 1 || v.0 == 2))
        {
            return Ok(Decision::keep("CURRENT_NATIVE_SUPPORT"));
        }
        let Some(snapshot) = frame
            .snapshot
            .filter(|s| (0..=200_000_000).contains(&(publication - s.complete)))
        else {
            return Ok(Decision::keep("MISSING_OR_STALE_CAMERA"));
        };
        let association = associate(
            Measurement {
                d_rel: d,
                y_rel: y,
                v_rel: v,
            },
            snapshot.objects,
            snapshot.count,
        )?;
        if association.0 == 1 || association.0 == 2 {
            return Ok(Decision::keep("CURRENT_A0_SUPPORTED_OR_AMBIGUOUS"));
        }
        Ok(Decision {
            exclude: true,
            reason: "MOVING_FIRST_SINGLETON_OFFPATH_CURRENTLY_UNSUPPORTED".into(),
            details: Some(Details {
                d_path: Number(d_path),
                world_longitudinal_speed_approx: Number(ego + v),
                current_a0: association,
                source_ns: member.1,
                snapshot_complete_ns: snapshot.complete,
                publication_ns: publication,
            }),
        })
    }
    pub fn publication_indices(
        &mut self,
        frame: &Frame<'_>,
        objects: &[Observation],
        publication: Option<i64>,
    ) -> Result<Vec<usize>, &'static str> {
        self.last_decisions.clear();
        if !self.enabled || objects.is_empty() || frame.can_error || frame.wrong_config {
            return Ok((0..objects.len()).collect());
        }
        let mut kept = vec![];
        for (i, obj) in objects.iter().enumerate() {
            if obj.age_scans != 1 || obj.members.len() != 1 {
                kept.push(i);
                continue;
            }
            let decision = self.verdict(frame, obj, publication, true, true)?;
            if decision.exclude {
                self.last_decisions.insert(obj.physical_track_id, decision);
            } else {
                kept.push(i)
            }
        }
        Ok(kept)
    }
    pub fn publication_view(
        &mut self,
        frame: &Frame<'_>,
        objects: &[Object],
        publication: Option<i64>,
    ) -> Result<Vec<Object>, &'static str> {
        let observations: Vec<_> = objects.iter().map(Observation::from).collect();
        Ok(self
            .publication_indices(frame, &observations, publication)?
            .into_iter()
            .map(|i| objects[i].clone())
            .collect())
    }
}
