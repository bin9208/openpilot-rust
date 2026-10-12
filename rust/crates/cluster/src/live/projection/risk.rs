use super::{Data, Projection};
use crate::models::ModelRiskPoint;

impl Projection {
    pub(super) fn model_meta(&mut self, meta: Data<'_>, model: Data<'_>) {
        self.state.model_confidence = model
            .field("confidence")
            .text()
            .or_else(|| self.state.model_confidence.clone());
        if let Some(value) = meta.field("engagedProb").float() {
            self.state.engaged_prob = Some(value.clamp(0.0, 1.0));
        }
        self.state.hard_brake_predicted =
            meta.flag("hardBrakePredicted", self.state.hard_brake_predicted);
        self.state.desire_state = meta.field("desireState").numbers(8, 0.0, 1.0);
        self.state.desire_prediction = meta
            .field("desirePrediction")
            .numbers(32, 0.0, 1.0)
            .chunks_exact(8)
            .map(<[f64]>::to_vec)
            .collect();
        let predictions = meta.field("disengagePredictions");
        self.state.risk_points = risk_points(predictions);
        self.state.brake_disengage_risk = maximum(predictions.field("brakeDisengageProbs"));
        self.state.gas_disengage_risk = maximum(predictions.field("gasDisengageProbs"));
        self.state.steer_override_risk = maximum(predictions.field("steerOverrideProbs"));
        self.state.hard_brake_risk = [
            "brake3MetersPerSecondSquaredProbs",
            "brake4MetersPerSecondSquaredProbs",
            "brake5MetersPerSecondSquaredProbs",
        ]
        .into_iter()
        .map(|field| maximum(predictions.field(field)))
        .fold(0.0, f64::max);
        self.state.gas_press_prob = maximum(predictions.field("gasPressProbs"));
        self.state.brake_press_prob = maximum(predictions.field("brakePressProbs"));
        self.state.disengage_risk = self
            .state
            .brake_disengage_risk
            .max(self.state.gas_disengage_risk)
            .max(self.state.steer_override_risk)
            .max(self.state.hard_brake_risk)
            .clamp(0.0, 1.0);
        self.state.lane_change_available_left = Some(meta.flag("laneChangeAvailableLeft", false));
        self.state.lane_change_available_right = Some(meta.flag("laneChangeAvailableRight", false));
        if let Some(value) = meta
            .field("modelTurnSpeed")
            .float()
            .filter(|value| *value > 0.0 && *value < 90.0)
        {
            self.state.model_turn_speed_kph = Some(value * 3.6);
        }
        self.state.left_lane_width_m = meta
            .field("laneWidthLeft")
            .float()
            .map(|value| value.clamp(0.0, 6.0));
        self.state.right_lane_width_m = meta
            .field("laneWidthRight")
            .float()
            .map(|value| value.clamp(0.0, 6.0));
        self.state.left_road_edge_distance_m = meta
            .field("distanceToRoadEdgeLeft")
            .float()
            .map(|value| value.clamp(0.0, 20.0));
        self.state.right_road_edge_distance_m = meta
            .field("distanceToRoadEdgeRight")
            .float()
            .map(|value| value.clamp(0.0, 20.0));
    }
}
fn maximum(value: Data<'_>) -> f64 {
    value
        .items()
        .into_iter()
        .filter_map(Data::float)
        .fold(0.0, f64::max)
}
fn risk_points(predictions: Data<'_>) -> Vec<ModelRiskPoint> {
    if !predictions.present() {
        return Vec::new();
    }
    let mut times = predictions.field("t").numbers(8, 0.0, 30.0);
    if times.is_empty() {
        times = vec![2.0, 4.0, 6.0, 8.0, 10.0];
    }
    let fields = [
        "brakeDisengageProbs",
        "gasDisengageProbs",
        "steerOverrideProbs",
        "brake3MetersPerSecondSquaredProbs",
        "brake4MetersPerSecondSquaredProbs",
        "brake5MetersPerSecondSquaredProbs",
        "gasPressProbs",
        "brakePressProbs",
    ]
    .map(|field| predictions.field(field).numbers(times.len(), 0.0, 1.0));
    times
        .into_iter()
        .enumerate()
        .map(|(index, t_s)| {
            let value = |field: usize| fields[field].get(index).copied().unwrap_or(0.0);
            ModelRiskPoint {
                t_s,
                brake_disengage: value(0),
                gas_disengage: value(1),
                steer_override: value(2),
                hard_brake_3: value(3),
                hard_brake_4: value(4),
                hard_brake_5: value(5),
                gas_press: value(6),
                brake_press: value(7),
            }
        })
        .collect()
}
