use crate::{model::Model, Error};
use openpilot_runtime_core::filters::FirstOrderFilter;

mod math;
mod update;

pub struct LanePathInput<'a> {
    pub speed: f64,
    pub times: &'a [f64],
    pub curve_speed: f64,
    pub lane_mode: bool,
    pub change_multiplier: f64,
    pub side_widths: [f64; 2],
}

#[derive(Debug)]
pub struct LanePlanner {
    times: Vec<f64>,
    x: Vec<f64>,
    left_y: Vec<f64>,
    right_y: Vec<f64>,
    left_edge: Vec<f64>,
    right_edge: Vec<f64>,
    probabilities: [f64; 2],
    deviations: [f64; 2],
    pub lane_change_probabilities: [f64; 2],
    width_filter: FirstOrderFilter,
    width_last: f64,
    side_filters: [FirstOrderFilter; 2],
    offset_filter: FirstOrderFilter,
    probability_count: u64,
    pub lane_width: f64,
    pub d_probability: f64,
    pub side_widths: [f64; 2],
    pub offset_total: f64,
}

impl Default for LanePlanner {
    fn default() -> Self {
        Self {
            times: vec![0.; 33],
            x: vec![0.; 33],
            left_y: vec![0.; 33],
            right_y: vec![0.; 33],
            left_edge: vec![0.; 33],
            right_edge: vec![0.; 33],
            probabilities: [0.; 2],
            deviations: [0.; 2],
            lane_change_probabilities: [0.; 2],
            width_filter: FirstOrderFilter::new(3.2, 3., 0.05, true),
            width_last: 3.2,
            side_filters: std::array::from_fn(|_| FirstOrderFilter::new(1., 1., 0.05, true)),
            offset_filter: FirstOrderFilter::new(0., 2., 0.05, true),
            probability_count: 0,
            lane_width: 3.2,
            d_probability: 0.,
            side_widths: [0.; 2],
            offset_total: 0.,
        }
    }
}

impl LanePlanner {
    pub fn parse_model(&mut self, model: &Model) -> Result<(), Error> {
        let lines = &model.lane_lines;
        if lines.len() >= 4 && lines[0].t.len() == 33 {
            self.times = math::paired(&lines[1].t, &lines[2].t, |a, b| (a + b) / 2.)?;
            self.x.clone_from(&lines[1].x);
            self.left_y.clone_from(&lines[1].y);
            self.right_y.clone_from(&lines[2].y);
            self.probabilities = [
                *model
                    .lane_line_probs
                    .get(1)
                    .ok_or(Error::Contract("left lane probability missing"))?,
                *model
                    .lane_line_probs
                    .get(2)
                    .ok_or(Error::Contract("right lane probability missing"))?,
            ];
            self.deviations = [
                *model
                    .lane_line_stds
                    .get(1)
                    .ok_or(Error::Contract("left lane deviation missing"))?,
                *model
                    .lane_line_stds
                    .get(2)
                    .ok_or(Error::Contract("right lane deviation missing"))?,
            ];
        }
        let edge = model
            .road_edges
            .first()
            .ok_or(Error::Contract("road edges missing"))?;
        if edge.t.len() == 33 {
            let right = model
                .road_edges
                .get(1)
                .ok_or(Error::Contract("right road edge missing"))?;
            let left_std = *model
                .road_edge_stds
                .first()
                .ok_or(Error::Contract("left edge deviation missing"))?;
            let right_std = *model
                .road_edge_stds
                .get(1)
                .ok_or(Error::Contract("right edge deviation missing"))?;
            self.left_edge = edge.y.iter().map(|value| value + left_std * 0.4).collect();
            self.right_edge = right
                .y
                .iter()
                .map(|value| value - right_std * 0.4)
                .collect();
        } else {
            self.left_edge.clone_from(&self.left_y);
            self.right_edge.clone_from(&self.right_y);
        }
        if !model.meta.desire_state.is_empty() {
            self.lane_change_probabilities = [
                *model
                    .meta
                    .desire_state
                    .get(3)
                    .ok_or(Error::Contract("left desire state missing"))?,
                *model
                    .meta
                    .desire_state
                    .get(4)
                    .ok_or(Error::Contract("right desire state missing"))?,
            ];
        }
        Ok(())
    }
}
