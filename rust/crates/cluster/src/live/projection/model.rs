use super::{points, Data, Projection};
use crate::config::MAX_ACCEL_MPS2;

impl Projection {
    pub(super) fn model(&mut self, model: Data<'_>, time: f64) {
        self.state.frame_age = model.field("frameAge").int().or(self.state.frame_age);
        if let Some(value) = model.field("frameDropPerc").float() {
            self.state.frame_drop_perc = Some(value.clamp(0.0, 100.0));
        }
        self.state.model_execution_time_ms = model
            .field("modelExecutionTime")
            .float()
            .map(|value| if value < 10.0 { value * 1000.0 } else { value });
        self.lanes.model(model, &mut self.state);
        let path = points::path_points(model);
        if !path.is_empty() {
            self.state.model_path = path;
        }
        self.action(model.field("action"));
        self.model_detections = points::model_leads(model);
        self.model_detection_time = time;
        let meta = model.field("meta");
        if meta.present() {
            self.change.meta(meta, true);
            self.model_meta(meta, model);
            self.state.lane_change_prob = self.change.probability;
        }
    }
    pub(super) fn driving_model(&mut self, model: Data<'_>) {
        self.lanes
            .driving(model.field("laneLineMeta"), &mut self.state);
        self.action(model.field("action"));
        let meta = model.field("meta");
        if meta.present() && !self.change.seen {
            self.change.meta(meta, false);
        }
    }
    fn action(&mut self, action: Data<'_>) {
        if !action.present() {
            return;
        }
        if let Some(value) = action
            .field("desiredVelocity")
            .float()
            .filter(|value| *value >= 0.0 && *value < 70.0)
        {
            self.state.planned_speed_kph = Some(value * 3.6);
        }
        if let Some(value) = action.field("desiredAcceleration").float() {
            self.state.planned_accel_mps2 = Some(value.clamp(-MAX_ACCEL_MPS2, MAX_ACCEL_MPS2));
        }
        if let Some(value) = action
            .field("desiredCurvature")
            .float()
            .filter(|value| value.abs() < 0.05)
        {
            self.state.planned_curvature_m_inv = Some(value);
        }
        self.state.should_stop = action.flag("shouldStop", self.state.should_stop);
    }
}
