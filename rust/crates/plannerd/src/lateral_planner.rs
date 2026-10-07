use crate::{
    car_state::CarState,
    lane_planner::{LanePathInput, LanePlanner},
    lateral_mpc::{LateralInput, LateralMpc},
    model::{Model, Trajectory},
    parameters::Parameters,
    path_geometry::yaw_from_path,
    Error,
};
use openpilot_cereal::log_capnp::Desire;
use openpilot_control_policy::math::{clip, divide, interp, maximum};
use std::path::Path;

mod output;
pub use output::Output;

#[derive(Clone, Copy, Debug, serde::Deserialize)]
pub struct Vehicle {
    pub wheelbase: f64,
    pub center_to_front: f64,
    pub mass: f64,
    pub tire_stiffness_rear: f64,
}

pub struct Input<'a> {
    pub car: &'a CarState,
    pub model: &'a Model,
    pub curvature: f64,
    pub curve_speed: f64,
    pub atc_active: bool,
}

pub struct LateralPlanner {
    factor1: f64,
    factor2: f64,
    last_warning: f64,
    pub invalid_count: u64,
    pub path: [[f64; 3]; 33],
    pub planned_speed: [f64; 33],
    pub ego_speed: f64,
    pub lane: LanePlanner,
    params_countdown: i32,
    pub lanes_active: bool,
    lane_speed: f64,
    path_offset: f64,
    lane_mode: bool,
    pub yaw: [f64; 33],
    pub yaw_rate: [f64; 33],
    pub times: Vec<f64>,
    pub y: [f64; 33],
    pub mpc: LateralMpc,
    pub initial: [f64; 4],
    pub curve_speed: f64,
    possible_count: u64,
    laneless_only: bool,
    weights: [f64; 5],
}

fn array(values: &[f64]) -> Result<[f64; 33], Error> {
    values
        .try_into()
        .map_err(|_| Error::Contract("lateral model trajectory dimensions"))
}
fn columns(value: &Trajectory) -> Result<[[f64; 3]; 33], Error> {
    let x = array(&value.x)?;
    let y = array(&value.y)?;
    let z = array(&value.z)?;
    Ok(std::array::from_fn(|i| [x[i], y[i], z[i]]))
}

impl LateralPlanner {
    pub fn load(
        vehicle: Vehicle,
        directory: &Path,
        parameters: &mut impl Parameters,
    ) -> Result<Self, Error> {
        let factor1 = vehicle.wheelbase - vehicle.center_to_front;
        let factor2 = divide(
            vehicle.center_to_front * vehicle.mass,
            vehicle.wheelbase * vehicle.tire_stiffness_rear,
        )?;
        let lane_speed = f64::from(parameters.integer("UseLaneLineSpeed")?);
        let path_offset = f64::from(parameters.integer("PathOffset")?) * 0.01;
        let mut mpc = LateralMpc::load(directory)?;
        mpc.reset([0.; 4])?;
        Ok(Self {
            factor1,
            factor2,
            last_warning: 0.,
            invalid_count: 0,
            path: [[0.; 3]; 33],
            planned_speed: [0.; 33],
            ego_speed: 1.,
            lane: LanePlanner::default(),
            params_countdown: 0,
            lanes_active: false,
            lane_speed,
            path_offset,
            lane_mode: false,
            yaw: [0.; 33],
            yaw_rate: [0.; 33],
            times: (0..33).map(f64::from).collect(),
            y: [0.; 33],
            mpc,
            initial: [0.; 4],
            curve_speed: 0.,
            possible_count: 0,
            laneless_only: true,
            weights: [1., 0.11, 0., 0.04, 700.],
        })
    }

    pub fn update(
        &mut self,
        input: Input<'_>,
        parameters: &mut impl Parameters,
        clock: &mut impl FnMut() -> f64,
    ) -> Result<bool, Error> {
        self.params_countdown -= 1;
        if self.params_countdown <= 0 {
            self.params_countdown = 10;
            self.lane_speed = input.car.use_lane_line_speed;
            self.path_offset = f64::from(parameters.integer("PathOffset")?) * 0.01;
            self.weights = [
                parameters.float("LatMpcPathCost")? * 0.01,
                parameters.float("LatMpcMotionCost")? * 0.01,
                parameters.float("LatMpcAccelCost")? * 0.01,
                parameters.float("LatMpcJerkCost")? * 0.01,
                parameters.float("LatMpcSteeringRateCost")?,
            ];
        }
        let speed = maximum(input.car.v_ego, 1.);
        let speed_kph = speed * 3.6;
        self.ego_speed = speed;
        self.curve_speed = input.curve_speed;
        let model = input.model;
        if model.position.x.len() == 33 && model.orientation.x.len() == 33 {
            self.path = columns(&model.position)?;
            self.times.clone_from(&model.position.t);
            self.yaw = array(&model.orientation.z)?;
            self.yaw_rate = array(&model.orientation_rate.z)?;
            self.planned_speed = columns(&model.velocity)?
                .map(|[x, y, z]| clip((x * x + y * y + z * z).sqrt(), 1., f64::INFINITY));
            self.ego_speed = self.planned_speed[0];
            if model.velocity.x[32] < model.velocity.x[0] * 0.7 {
                self.possible_count = 0;
                self.laneless_only = true;
            } else {
                self.possible_count += 1;
                if self.possible_count > 20 {
                    self.laneless_only = false;
                }
            }
        }
        self.lane.parse_model(model)?;
        if self.lane_speed == 0. || self.laneless_only {
            self.lane_mode = false;
        } else if speed_kph >= self.lane_speed + 2. {
            self.lane_mode = true;
        } else if speed_kph < self.lane_speed - 2. {
            self.lane_mode = false;
        }
        self.lanes_active = self.lane.apply(
            LanePathInput {
                speed,
                times: &self.times,
                curve_speed: self.curve_speed,
                lane_mode: self.lane_mode,
                change_multiplier: if model.meta.desire != Desire::None || input.atc_active {
                    0.
                } else {
                    1.
                },
                side_widths: [model.meta.lane_width_left, model.meta.lane_width_right],
            },
            parameters,
            &mut self.path,
        )?;
        if self.lanes_active {
            let yaw = yaw_from_path(&self.path, &self.planned_speed)?;
            self.yaw = yaw.yaw;
            self.yaw_rate = yaw.rate;
        }
        for point in &mut self.path {
            point[1] += self.path_offset;
        }
        self.mpc.set_weights(self.weights)?;
        self.y = self.path.map(|row| row[1]);
        let params = self.planned_speed.map(|speed| {
            [
                speed,
                clip(
                    self.factor1 - (self.factor2 * speed.powi(2)),
                    0.,
                    f64::INFINITY,
                ),
            ]
        });
        self.mpc.run(LateralInput {
            initial: self.initial,
            parameters: &params,
            y: &self.y,
            heading: &self.yaw,
            yaw_rate: &self.yaw_rate,
        })?;
        self.initial[3] = interp(
            0.05,
            self.times
                .get(..33)
                .ok_or(Error::Contract("short lateral time trajectory"))?,
            &self.mpc.x.map(|row| row[3]),
        )?;
        let nan = self.mpc.x.iter().any(|row| row[3].is_nan());
        let now = clock();
        let mut warning = false;
        if nan || self.mpc.status != 0 {
            self.initial = [0.; 4];
            self.mpc.reset(self.initial)?;
            self.initial[3] = input.curvature * self.ego_speed;
            if now > self.last_warning + 5. {
                self.last_warning = now;
                warning = true;
            }
        }
        self.invalid_count = if self.mpc.cost > 1e6 || nan {
            self.invalid_count + 1
        } else {
            0
        };
        Ok(warning)
    }
}
