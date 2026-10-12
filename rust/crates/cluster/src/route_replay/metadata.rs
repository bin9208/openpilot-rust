use super::Metadata;
use crate::live::{projection::Data, Projection};

impl Metadata {
    pub(super) fn parser_initial() -> Self {
        let source =
            std::env::var("CLUSTER_ROUTE_CUTIN_RADAR_SOURCE").unwrap_or_else(|_| "corner".into());
        let source = if source == "front" { "FRONT" } else { "CORNER" };
        let sensitivity = std::env::var("CLUSTER_ROUTE_CUTIN_SENSITIVITY")
            .ok()
            .and_then(|text| openpilot_runtime_core::python_float::parse(&text))
            .unwrap_or(50.0);
        let sensitivity = crate::scene::math::clamp(sensitivity, 0.0, 100.0);
        Self {
            cutin_debug_text: format!("NEW CUTIN {source} S{sensitivity:.0}: waiting"),
            ..Self::default()
        }
    }
    pub(super) fn observe(&mut self, name: &str, data: Data<'_>, projection: &Projection) {
        match name {
            "modelV2" => {
                let lines = data.field("laneLines");
                let left = lines.index(1).field("y").index(0).float();
                let right = lines.index(2).field("y").index(0).float();
                if left.zip(right).is_some_and(|(left, right)| right > left) {
                    self.lane_position_source = "modelV2".into();
                }
                if !projection.state.model_path.is_empty() {
                    self.model_path_source = "modelV2.position".into();
                }
                if data.field("meta").present() {
                    self.lane_change_source = "modelV2".into();
                }
            }
            "drivingModelData" => {
                let meta = data.field("laneLineMeta");
                if meta
                    .field("leftY")
                    .float()
                    .zip(meta.field("rightY").float())
                    .is_some_and(|(left, right)| right > left)
                {
                    self.lane_position_source = "drivingModelData".into();
                }
                if data.field("meta").present() && self.lane_change_source != "modelV2" {
                    self.lane_change_source = "drivingModelData".into();
                }
            }
            "lateralPlan" => {
                if self.lane_change_source != "modelV2" {
                    self.lane_change_source = "lateralPlan".into();
                }
                if let Some(curvature) = data
                    .field("curvatures")
                    .index(0)
                    .float()
                    .filter(|value| value.abs() < 0.05)
                {
                    self.road_curvature = Some(curvature);
                    self.road_curvature_source = "lateralPlan".into();
                }
            }
            "controlsState" if self.road_curvature_source != "lateralPlan" => {
                for (field, source) in [
                    ("desiredCurvature", "controlsState.desired"),
                    ("curvature", "controlsState"),
                ] {
                    if let Some(curvature) =
                        data.field(field).float().filter(|value| value.abs() < 0.05)
                    {
                        self.road_curvature = Some(curvature);
                        self.road_curvature_source = source.into();
                        break;
                    }
                }
            }
            "radarState" => self.cutin_debug_text = projection.recorded_cutin_debug().into(),
            _ => (),
        }
    }
}
