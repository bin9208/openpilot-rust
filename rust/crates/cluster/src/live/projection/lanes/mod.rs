mod geometry;
mod markings;
mod replay;
use super::{change::Animation, points, Data};
use crate::models::{ClusterUiState, ModelPathPoint};
pub use replay::Snapshot;

pub(crate) struct Lanes {
    pub positions: [Option<f64>; 4],
    probabilities: [f64; 4],
    pub points: Vec<Vec<ModelPathPoint>>,
    pub edges: Vec<Vec<ModelPathPoint>>,
    edge_positions: [Option<f64>; 2],
    styles: [String; 2],
    colors: [Option<[u8; 3]>; 2],
}
impl Default for Lanes {
    fn default() -> Self {
        Self {
            positions: [None; 4],
            probabilities: [0.0, 1.0, 1.0, 0.0],
            points: Vec::new(),
            edges: Vec::new(),
            edge_positions: [None; 2],
            styles: ["solid".into(), "solid".into()],
            colors: [None; 2],
        }
    }
}
impl Lanes {
    pub fn model(&mut self, model: Data<'_>, state: &mut ClusterUiState) {
        self.points = model
            .field("laneLines")
            .items()
            .into_iter()
            .map(points::line_points)
            .collect();
        let lines = model.field("laneLines").items();
        if lines.len() >= 3 {
            let left = lines[1].field("y").index(0).float();
            let right = lines[2].field("y").index(0).float();
            if let Some((left, right)) = left.zip(right).filter(|(left, right)| right > left) {
                self.positions[1] = Some(left);
                self.positions[2] = Some(right);
                self.width(state);
            }
        }
        if lines.len() >= 4 {
            self.positions[0] = lines[0].field("y").index(0).float();
            self.positions[3] = lines[3].field("y").index(0).float();
        }
        let probabilities = model.field("laneLineProbs").items();
        if probabilities.len() >= 3 {
            self.probabilities[1] = probabilities[1].float().unwrap_or(0.0).clamp(0.0, 1.0);
            self.probabilities[2] = probabilities[2].float().unwrap_or(0.0).clamp(0.0, 1.0);
        }
        if probabilities.len() >= 4 {
            self.probabilities[0] = probabilities[0].float().unwrap_or(0.0).clamp(0.0, 1.0);
            self.probabilities[3] = probabilities[3].float().unwrap_or(0.0).clamp(0.0, 1.0);
        }
        let edges = model.field("roadEdges").items();
        self.edges = edges
            .iter()
            .copied()
            .take(2)
            .map(points::line_points)
            .collect();
        if edges.len() >= 2 {
            self.edge_positions = [
                edges[0].field("y").index(0).float(),
                edges[1].field("y").index(0).float(),
            ];
        }
        let standard_deviations = model.field("roadEdgeStds").items();
        if standard_deviations.len() >= 2 {
            let confidence = |index: usize| {
                standard_deviations[index]
                    .float()
                    .map_or(0.0, |value| (1.0 - value / 2.0).clamp(0.0, 1.0))
            };
            state.left_road_edge_confidence = confidence(0);
            state.right_road_edge_confidence = confidence(1);
        }
    }
    pub fn driving(&mut self, meta: Data<'_>, state: &mut ClusterUiState) {
        let left = meta.field("leftY").float();
        let right = meta.field("rightY").float();
        if left.zip(right).is_some_and(|(left, right)| right > left) {
            self.positions[1] = left;
            self.positions[2] = right;
            self.width(state);
        }
        self.probabilities[1] = meta
            .number("leftProb", self.probabilities[1])
            .clamp(0.0, 1.0);
        self.probabilities[2] = meta
            .number("rightProb", self.probabilities[2])
            .clamp(0.0, 1.0);
    }
    fn width(&self, state: &mut ClusterUiState) {
        if let Some((left, right)) = self.positions[1]
            .zip(self.positions[2])
            .filter(|(left, right)| right > left)
        {
            state.lane_width_m = (right - left).clamp(2.4, 4.6);
        }
    }
    pub fn car(&mut self, car: Data<'_>) {
        for (index, field) in ["leftLaneLine", "rightLaneLine"].into_iter().enumerate() {
            let Some(code) = car.field(field).int() else {
                continue;
            };
            self.styles[index] = if code >= 0 && code % 10 == 0 {
                "dashed".into()
            } else {
                "solid".into()
            };
            self.colors[index] = match code / 10 {
                1 if code >= 10 => Some(crate::config::WHITE),
                2 if code >= 10 => Some(crate::config::YELLOW),
                _ => None,
            };
            if code < 0 {
                self.probabilities[index + 1] = 0.0;
            }
        }
    }
    pub fn frame(&self, state: &mut ClusterUiState, animation: &Animation) {
        let geometry = self.geometry(state.lane_width_m);
        self.frame_geometry(state, animation, &geometry);
    }
    fn frame_geometry(
        &self,
        state: &mut ClusterUiState,
        animation: &Animation,
        geometry: &geometry::Geometry,
    ) {
        state.camera_lane_center_offset_m = geometry.center;
        state.extra_left_lane_visible = geometry.extra[0];
        state.extra_right_lane_visible = geometry.extra[1];
        state.left_road_edge_offset = geometry.edges[0].map(|offset| {
            offset
                + if animation.animated {
                    animation.grid
                } else {
                    0.0
                }
        });
        state.right_road_edge_offset = geometry.edges[1].map(|offset| {
            offset
                + if animation.animated {
                    animation.grid
                } else {
                    0.0
                }
        });
        state.left_road_edge_points = if geometry.edges[0].is_some() {
            self.edges.first().cloned().unwrap_or_default()
        } else {
            Vec::new()
        };
        state.right_road_edge_points = if geometry.edges[1].is_some() {
            self.edges.get(1).cloned().unwrap_or_default()
        } else {
            Vec::new()
        };
        state.left_road_edge_lateral_shift_m = markings::shift(
            &state.left_road_edge_points,
            state.lane_width_m,
            geometry.center,
            state.left_road_edge_offset,
            animation,
        );
        state.right_road_edge_lateral_shift_m = markings::shift(
            &state.right_road_edge_points,
            state.lane_width_m,
            geometry.center,
            state.right_road_edge_offset,
            animation,
        );
        state.lanes = self.markings(state, geometry, animation);
    }
}
