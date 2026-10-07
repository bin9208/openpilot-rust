use crate::{
    carrot::{self, CarrotPlanner},
    coasting::CruiseCoastingPlan,
    longitudinal_mpc::{LongitudinalMpc, Reference},
    model::Model,
    parameters::Parameters,
    radar::Radar,
    Error,
};
use openpilot_cereal::car_capnp::car_control::actuators::LongControlState;
use openpilot_control_policy::{drive::TIMES, math::interp};
use openpilot_runtime_core::filters::FirstOrderFilter;
use std::path::Path;

mod coasting;
mod target;
mod update;

#[derive(Clone, Copy, Debug, serde::Deserialize)]
pub struct Vehicle {
    pub longitudinal_control: bool,
    pub volkswagen_meb: bool,
}

pub struct Input<'a> {
    pub carrot: carrot::Input<'a>,
    pub enabled: bool,
    pub experimental: bool,
    pub control_state: LongControlState,
    pub force_deceleration: bool,
    pub desired_curvature: f64,
    pub curvature: f64,
    pub coasting_checks: bool,
    pub navigation_seen: bool,
}

pub struct LongitudinalPlanner {
    vehicle: Vehicle,
    pub mpc: LongitudinalMpc,
    pub fcw: bool,
    dt: f64,
    pub desired_acceleration: f64,
    desired_speed: FirstOrderFilter,
    pub target_acceleration: f64,
    pub base_acceleration: f64,
    pub target_speed: f64,
    pub target_jerk: f64,
    pub should_stop: bool,
    pub lead_preview: f64,
    pub preview_action_time: f64,
    pub preview_acceleration: f64,
    track_ids: [i32; 2],
    pub track_frames: [u64; 2],
    pub speed_trajectory: [f64; 17],
    pub accel_trajectory: [f64; 17],
    pub jerk_trajectory: [f64; 17],
    pub cluster_ratio: f64,
    reset_decel_timer: u64,
    reset_decel_start: f64,
    pub cruise_kph: f64,
    coasting: CruiseCoastingPlan,
    pub coasting_percent: i32,
    coasting_param_time: f64,
    pub coasting_target: f64,
}

impl LongitudinalPlanner {
    pub fn load(
        vehicle: Vehicle,
        directory: &Path,
        speed: f64,
        acceleration: f64,
        dt: f64,
    ) -> Result<Self, Error> {
        Ok(Self {
            vehicle,
            mpc: LongitudinalMpc::load(directory, crate::types::PlannerMode::Acc, dt)?,
            fcw: false,
            dt,
            desired_acceleration: acceleration,
            desired_speed: FirstOrderFilter::new(speed, 2., dt, true),
            target_acceleration: 0.,
            base_acceleration: 0.,
            target_speed: 0.,
            target_jerk: 0.,
            should_stop: false,
            lead_preview: 0.,
            preview_action_time: 0.,
            preview_acceleration: 0.,
            track_ids: [-1; 2],
            track_frames: [0; 2],
            speed_trajectory: [0.; 17],
            accel_trajectory: [0.; 17],
            jerk_trajectory: [0.; 17],
            cluster_ratio: 1.,
            reset_decel_timer: 0,
            reset_decel_start: 0.,
            cruise_kph: 0.,
            coasting: CruiseCoastingPlan::default(),
            coasting_percent: 0,
            coasting_param_time: 1.,
            coasting_target: 0.,
        })
    }

    pub fn parse_model(model: &Model) -> Result<Reference, Error> {
        let mut result = Reference {
            x: [0.; 13],
            speed: [0.; 13],
            acceleration: [0.; 13],
            jerk: [0.; 13],
        };
        if model.position.x.len() == 33
            && model.velocity.x.len() == 33
            && model.acceleration.x.len() == 33
        {
            for (i, time) in crate::longitudinal_mpc::TIMES.iter().enumerate() {
                result.x[i] = interp(*time, &TIMES, &model.position.x)?;
                result.speed[i] = interp(*time, &TIMES, &model.velocity.x)?;
                result.acceleration[i] = interp(*time, &TIMES, &model.acceleration.x)?;
            }
        }
        Ok(result)
    }

    fn update_tracks(&mut self, radar: &Radar) {
        for (index, lead) in [&radar.lead_one, &radar.lead_two].into_iter().enumerate() {
            let id = if lead.status && lead.radar && lead.radar_track_id >= 0 {
                lead.radar_track_id
            } else {
                -1
            };
            if id >= 0 && id == self.track_ids[index] {
                self.track_frames[index] += 1;
            } else if id >= 0 {
                self.track_ids[index] = id;
                self.track_frames[index] = 1;
            } else {
                self.track_ids[index] = -1;
                self.track_frames[index] = 0;
            }
        }
    }

    fn set_speed_filter(&mut self, value: f64) {
        self.desired_speed = FirstOrderFilter::new(value, 2., self.dt, true);
    }
}
