//! Causal, immutable receive context held independently from the CAN/ego join.
use super::{
    birth_b5::{Model, Number},
    model_snapshot::{Input, Snapshot},
    physical::VisionCue,
    provider::Provider,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[derive(Clone, Deserialize, Serialize)]
pub struct Angular {
    pub valid: bool,
    pub z: Number,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pose {
    #[serde(rename = "inputsOK")]
    pub inputs_ok: bool,
    #[serde(rename = "sensorsOK")]
    pub sensors_ok: bool,
    pub angular_velocity_device: Option<Angular>,
}
pub struct Message<T> {
    pub value: Option<Arc<T>>,
    pub timestamp: i64,
}
impl<T> Default for Message<T> {
    fn default() -> Self {
        Self {
            value: None,
            timestamp: 0,
        }
    }
}
impl<T> Clone for Message<T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            timestamp: self.timestamp,
        }
    }
}
struct CachedModel {
    source: Arc<Input>,
    timestamp: i64,
    snapshot: Snapshot,
    geometry: Arc<Model>,
}
#[derive(Default)]
pub struct Cache {
    pub last_pose: Message<Pose>,
    pub last_model: Message<Input>,
    pose_yaw: Option<(Arc<Pose>, Option<f64>)>,
    models: Vec<CachedModel>,
    pub path_ns: Option<i64>,
    pub path_source_ns: i64,
    pub path: Vec<(f64, f64)>,
}
pub struct Pending {
    pub now: i64,
    pub yaw: Option<f64>,
    pub cues: Vec<VisionCue>,
    pub path: Vec<(f64, f64)>,
    pub path_ns: i64,
    pub path_source_ns: i64,
}
impl Cache {
    pub fn set(
        &mut self,
        provider: &mut Provider,
        now: i64,
        mut pose: Message<Pose>,
        mut model: Message<Input>,
    ) -> Pending {
        if pose.value.is_some() && pose.timestamp > now {
            pose = self.last_pose.clone();
        } else {
            self.last_pose = pose.clone();
        }
        if model.value.is_some() && model.timestamp > now {
            model = self.last_model.clone();
        } else {
            self.last_model = model.clone();
        }
        let mut yaw = None;
        if let Some(value) = pose
            .value
            .as_ref()
            .filter(|_| (0..=200_000_000).contains(&(now - pose.timestamp)))
        {
            if self
                .pose_yaw
                .as_ref()
                .is_none_or(|(prior, _)| !Arc::ptr_eq(prior, value))
            {
                let angle = value
                    .angular_velocity_device
                    .as_ref()
                    .filter(|v| value.inputs_ok && value.sensors_ok && v.valid && v.z.0.is_finite())
                    .map(|v| -v.z.0);
                self.pose_yaw = Some((Arc::clone(value), angle));
            }
            yaw = self.pose_yaw.as_ref().and_then(|(_, angle)| *angle);
        }
        provider.b5_birth_defer.ingest_pose(pose.timestamp, yaw);
        let mut cues = vec![];
        let mut path = vec![];
        let mut source_ns = 0;
        if let Some(value) = model
            .value
            .as_ref()
            .filter(|_| (0..=200_000_000).contains(&(now - model.timestamp)))
        {
            let index = if let Some(index) = self
                .models
                .iter()
                .position(|m| Arc::ptr_eq(&m.source, value) && m.timestamp == model.timestamp)
            {
                index
            } else {
                let snapshot = Snapshot::new(value);
                let geometry = Arc::new(snapshot.geometry().clone());
                if self.models.len() == 2 {
                    self.models.remove(0);
                }
                self.models.push(CachedModel {
                    source: Arc::clone(value),
                    timestamp: model.timestamp,
                    snapshot,
                    geometry,
                });
                self.models.len() - 1
            };
            let cached = &self.models[index];
            provider
                .road_edge_filter
                .ingest_model(Some(Arc::clone(&cached.geometry)), model.timestamp);
            provider
                .b5_birth_defer
                .ingest_model(Some(&cached.geometry), model.timestamp);
            provider
                .mirror_m3_shadow
                .ingest_model(Some(&cached.geometry), model.timestamp);
            cues = cached.snapshot.cues().collect();
            if self.path_ns != Some(model.timestamp) {
                self.path = cached
                    .geometry
                    .position
                    .as_ref()
                    .map(|p| p.x.iter().zip(&p.y).map(|(x, y)| (x.0, y.0)).collect())
                    .unwrap_or_default();
                self.path_source_ns = cached.geometry.source_ns;
                self.path_ns = Some(model.timestamp);
            }
            path = self.path.clone();
            source_ns = self.path_source_ns;
        } else {
            provider.road_edge_filter.invalidate_model();
        }
        Pending {
            now,
            yaw,
            cues,
            path,
            path_ns: model.timestamp,
            path_source_ns: source_ns,
        }
    }
    pub fn cached_models(&self) -> Vec<(&Arc<Input>, i64, &Snapshot)> {
        self.models
            .iter()
            .map(|m| (&m.source, m.timestamp, &m.snapshot))
            .collect()
    }
    pub fn cached_pose(&self) -> Option<(&Arc<Pose>, Option<f64>)> {
        self.pose_yaw.as_ref().map(|(p, y)| (p, *y))
    }
}
