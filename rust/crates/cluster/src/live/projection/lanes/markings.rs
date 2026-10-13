use super::{
    geometry::{inside, Geometry},
    Animation, Lanes,
};
use crate::{
    config::{BLUE, BLUE_SOFT, WHITE},
    models::{ClusterUiState, LaneMarking, ModelPathPoint},
};
use num_traits::ToPrimitive;

pub(super) fn shift(
    points: &[ModelPathPoint],
    width: f64,
    center: Option<f64>,
    baseline: Option<f64>,
    animation: &Animation,
) -> f64 {
    let Some(first) = points.first() else {
        return 0.0;
    };
    if animation.animated {
        if let Some(offset) = baseline {
            return offset * width.max(0.1) - first.lateral_m;
        }
    }
    -center.unwrap_or(0.0) + animation.grid * width.max(0.1)
}
impl Lanes {
    pub(super) fn markings(
        &self,
        state: &ClusterUiState,
        geometry: &Geometry,
        animation: &Animation,
    ) -> Vec<LaneMarking> {
        let inner = if animation.animated {
            [-0.5 + animation.grid, 0.5 + animation.grid]
        } else {
            geometry.inner.map(|offset| offset + animation.grid)
        };
        let visible = geometry
            .visible
            .map(|visible| visible || animation.animated);
        let edges = [state.left_road_edge_offset, state.right_road_edge_offset];
        let direction = state.lane_change.as_deref();
        let mut markings = self.model_markings(state, geometry, animation, inner, visible);
        if !markings.is_empty() {
            return markings;
        }
        let make = |index: usize,
                    offset: f64,
                    color: [u8; 3],
                    style: String,
                    visible: bool,
                    width: i64| {
            let points = self.points.get(index).cloned().unwrap_or_default();
            let lateral_shift = shift(
                &points,
                state.lane_width_m,
                geometry.center,
                Some(offset),
                animation,
            );
            LaneMarking {
                offset,
                color,
                style,
                visible: visible && inside(offset, edges),
                width,
                model_points: points,
                model_lateral_shift_m: lateral_shift,
            }
        };
        if animation.animated && direction == Some("left") {
            markings.push(make(
                0,
                inner[0] - 1.0,
                BLUE_SOFT,
                "solid".into(),
                geometry.extra[0] || animation.animated,
                5,
            ));
        }
        markings.push(make(
            1,
            inner[0],
            self.colors[0].unwrap_or(BLUE),
            self.styles[0].clone(),
            visible[0],
            7,
        ));
        markings.push(make(
            2,
            inner[1],
            self.colors[1].unwrap_or(BLUE),
            self.styles[1].clone(),
            visible[1],
            7,
        ));
        if animation.animated && direction == Some("right") {
            markings.push(make(
                3,
                inner[1] + 1.0,
                BLUE_SOFT,
                "dashed".into(),
                geometry.extra[1] || animation.animated,
                5,
            ));
        }
        markings
    }
    fn model_markings(
        &self,
        state: &ClusterUiState,
        geometry: &Geometry,
        animation: &Animation,
        inner: [f64; 2],
        visible: [bool; 2],
    ) -> Vec<LaneMarking> {
        let edges = [state.left_road_edge_offset, state.right_road_edge_offset];
        let direction = state.lane_change.as_deref();
        let mut markings = Vec::new();
        for (index, points) in self
            .points
            .iter()
            .enumerate()
            .filter(|(_, points)| !points.is_empty())
        {
            let (offset, color, style, visible, width) = if index == 1 || index == 2 {
                let index = index - 1;
                (
                    inner[index],
                    self.colors[index].unwrap_or(BLUE),
                    self.styles[index].clone(),
                    visible[index],
                    7,
                )
            } else {
                let offset = geometry.center.map_or_else(
                    || {
                        if index < 1 {
                            inner[0] - (1 - index).to_f64().unwrap_or(1.0)
                        } else {
                            inner[1] + index.saturating_sub(2).to_f64().unwrap_or(1.0)
                        }
                    },
                    |center| (points[0].lateral_m - center) / state.lane_width_m + animation.grid,
                );
                let color = if (index == 0 && direction == Some("left"))
                    || (index == 3 && direction == Some("right"))
                {
                    BLUE_SOFT
                } else {
                    WHITE
                };
                let visible = match index {
                    0 => geometry.extra[0] || (animation.animated && direction == Some("left")),
                    3 => geometry.extra[1] || (animation.animated && direction == Some("right")),
                    _ => true,
                };
                (
                    offset,
                    color,
                    if index < 2 {
                        "solid".into()
                    } else {
                        "dashed".into()
                    },
                    visible,
                    5,
                )
            };
            markings.push(LaneMarking {
                offset,
                color,
                style,
                visible: visible && inside(offset, edges),
                width,
                model_points: points.clone(),
                model_lateral_shift_m: shift(
                    points,
                    state.lane_width_m,
                    geometry.center,
                    Some(offset),
                    animation,
                ),
            });
        }
        markings
    }
}
