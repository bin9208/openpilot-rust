use super::Data;
use crate::{config::MODEL_DIRECT_LANE_RECENTER_SECONDS, models::ClusterUiState};

pub(super) struct Change {
    pub seen: bool,
    pub probability: f64,
    state: String,
    direction: String,
    desires: [f64; 2],
    active: Option<String>,
    last_progress: f64,
    recenter: Option<(String, f64, f64)>,
    pub(super) recenter_start: f64,
}
impl Default for Change {
    fn default() -> Self {
        Self {
            seen: false,
            probability: 1.0,
            state: "off".into(),
            direction: "none".into(),
            desires: [0.0; 2],
            active: None,
            last_progress: 0.0,
            recenter: None,
            recenter_start: 1.0,
        }
    }
}
pub(crate) struct Animation {
    pub grid: f64,
    pub animated: bool,
}
impl Change {
    pub fn meta(&mut self, meta: Data<'_>, from_model: bool) {
        self.state = meta
            .field("laneChangeState")
            .text()
            .unwrap_or_else(|| self.state.clone());
        self.direction = meta
            .field("laneChangeDirection")
            .text()
            .unwrap_or_else(|| self.direction.clone());
        if from_model {
            self.seen = true;
            self.probability = meta
                .number("laneChangeProb", self.probability)
                .clamp(0.0, 1.0);
            if meta.field("desireState").items().len() > 4 {
                for (index, slot) in self.desires.iter_mut().enumerate() {
                    if let Some(value) = meta.field("desireState").index(index + 3).float() {
                        *slot = value.clamp(0.0, 1.0);
                    }
                }
            }
        }
    }
    fn values(&mut self, time: f64) -> (Option<String>, &'static str, f64, f64) {
        if !self.seen {
            self.clear();
            return (None, "idle", 0.0, 1.0);
        }
        let direction =
            matches!(self.direction.as_str(), "left" | "right").then(|| self.direction.clone());
        if direction.is_none() || self.state == "off" {
            if self.recenter.is_none() && self.active.is_some() && self.last_progress >= 0.65 {
                self.recenter = self
                    .active
                    .take()
                    .map(|direction| (direction, time, self.last_progress.clamp(0.0, 1.0)));
                self.last_progress = 0.0;
            }
            if let Some((direction, started, initial)) = &self.recenter {
                let progress = ((time - started).max(0.0) / MODEL_DIRECT_LANE_RECENTER_SECONDS)
                    .clamp(0.0, 1.0);
                if progress < 1.0 {
                    return (Some(direction.clone()), "recentering", progress, *initial);
                }
            }
            self.clear();
            return (None, "idle", 0.0, 1.0);
        }
        self.recenter = None;
        self.active.clone_from(&direction);
        if self.state == "preLaneChange" {
            self.last_progress = 0.0;
            return (direction, "preparing", 0.0, 1.0);
        }
        let progress = match self.state.as_str() {
            "laneChangeStarting" => 1.0 - self.desires[usize::from(self.direction == "right")],
            "laneChangeFinishing" => 1.0,
            _ => 0.0,
        };
        self.last_progress = progress;
        (direction, "changing", progress, 1.0)
    }
    fn clear(&mut self) {
        self.active = None;
        self.last_progress = 0.0;
        self.recenter = None;
    }
    pub fn frame(&mut self, state: &mut ClusterUiState, time: f64, observed: f64) -> Animation {
        let (direction, phase, progress, initial) = self.values(time);
        state.lane_change.clone_from(&direction);
        state.highlight_lane = direction;
        state.lane_change_phase = phase.into();
        state.lane_change_progress = progress;
        state.lane_change_prob = self.probability;
        self.recenter_start = initial;
        animate(state, initial, observed)
    }
}

pub(crate) fn animate(state: &mut ClusterUiState, initial: f64, observed: f64) -> Animation {
    if !matches!(state.lane_change.as_deref(), Some("left" | "right")) {
        state.ego_lane_offset = observed;
        state.highlight_lane_offset = None;
        state.road_view_lane_position = 0.0;
        return Animation {
            grid: 0.0,
            animated: false,
        };
    }
    let progress = state.lane_change_progress;
    let sign = if state.lane_change.as_deref() == Some("left") {
        -1.0
    } else {
        1.0
    };
    let mut grid = 0.0;
    let animated = match state.lane_change_phase.as_str() {
        "preparing" => {
            state.ego_lane_offset = 0.0;
            state.highlight_lane_offset = Some(sign);
            true
        }
        "changing" => {
            state.ego_lane_offset = sign * progress.clamp(0.0, 1.0);
            state.highlight_lane_offset = Some(sign);
            true
        }
        "recentering" => {
            let blend = progress * progress * (3.0 - 2.0 * progress);
            let initial = initial * initial * (3.0 - 2.0 * initial);
            grid = -sign * blend;
            state.ego_lane_offset = sign * initial * (1.0 - blend) + observed * blend;
            state.highlight_lane_offset = None;
            true
        }
        _ => {
            state.ego_lane_offset = observed;
            state.highlight_lane_offset = None;
            false
        }
    };
    state.road_view_lane_position = grid;
    Animation { grid, animated }
}
