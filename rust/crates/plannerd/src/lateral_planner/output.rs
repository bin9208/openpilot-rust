use super::LateralPlanner;
use openpilot_control_policy::math::{clip, maximum};

#[derive(Debug, serde::Serialize)]
pub struct Output {
    pub path_points: Vec<f64>,
    pub headings: [f64; 17],
    pub distances: [f64; 17],
    pub curvatures: [f64; 17],
    pub curvature_rates: [f64; 17],
    pub solution_valid: bool,
    pub use_lane_lines: bool,
    pub lane_width: f64,
    pub position: [Vec<f64>; 3],
    pub debug_text: String,
}

impl LateralPlanner {
    pub fn output(&self) -> Output {
        let curvature_divisor = maximum(self.ego_speed, 6.);
        let mode = if self.lanes_active {
            "lanemode"
        } else {
            "laneless"
        };
        let tail = if self.lanes_active {
            format!(
                "offset={:.1}cm turn={:.0}km/h",
                self.lane.offset_total * 100.,
                clip(self.curve_speed, -200., 200.)
            )
        } else {
            String::new()
        };
        Output {
            path_points: self.y.to_vec(),
            headings: std::array::from_fn(|i| self.mpc.x[i][2]),
            distances: std::array::from_fn(|i| self.mpc.x[i][0]),
            curvatures: std::array::from_fn(|i| {
                self.mpc.x[i][3]
                    / if self.planned_speed[i].is_nan() {
                        f64::NAN
                    } else {
                        maximum(self.planned_speed[i], 6.)
                    }
            }),
            curvature_rates: std::array::from_fn(|i| {
                if i < 16 {
                    self.mpc.u[i][0] / curvature_divisor
                } else {
                    0.
                }
            }),
            solution_valid: self.invalid_count < 2,
            use_lane_lines: self.lanes_active,
            lane_width: self.lane.lane_width,
            position: [
                self.mpc.x.iter().map(|row| row[0]).collect(),
                self.mpc.x.iter().map(|row| row[1]).collect(),
                self.path.iter().map(|row| row[2]).collect(),
            ],
            debug_text: format!(
                "{mode} | {:.1}m | {:.1}m | {:.1}m | {tail}",
                self.lane.side_widths[0], self.lane.lane_width, self.lane.side_widths[1]
            ),
        }
    }
}
