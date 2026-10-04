use super::{CarrotPlanner, Input, LongitudinalPlanner, Parameters, TIMES};
use crate::{
    longitudinal_mpc,
    preview::{self, PreviewInput},
    types::{MpcSource, PlannerMode},
    Error,
};
use openpilot_control_policy::math::{interp, minimum};

impl LongitudinalPlanner {
    pub(super) fn targets(
        &mut self,
        input: &Input<'_>,
        carrot: &CarrotPlanner,
        reset: bool,
        parameters: &mut impl Parameters,
    ) -> Result<(), Error> {
        for (i, time) in TIMES[..17].iter().enumerate() {
            self.speed_trajectory[i] = interp(*time, &longitudinal_mpc::TIMES, &self.mpc.speed)?;
            self.accel_trajectory[i] =
                interp(*time, &longitudinal_mpc::TIMES, &self.mpc.acceleration)?;
            self.jerk_trajectory[i] =
                interp(*time, &longitudinal_mpc::TIMES[..12], &self.mpc.jerk)?;
        }
        let car = input.carrot.car;
        let crash_threshold = if self.vehicle.volkswagen_meb { 15 } else { 2 };
        let suppress = self.vehicle.volkswagen_meb && (car.gas_pressed || car.a_ego > 1.);
        self.fcw = self.mpc.crash_count > crash_threshold && !car.standstill && !reset && !suppress;
        let previous = self.desired_acceleration;
        self.desired_acceleration = interp(self.dt, &TIMES[..17], &self.accel_trajectory)?;
        self.set_speed_filter(
            self.desired_speed.value() + self.dt * (self.desired_acceleration + previous) / 2.,
        );
        let delay = parameters.float("LongActuatorDelay")? * 0.01;
        let stopping_speed = parameters.float("VEgoStopping")? * 0.01;
        let action_time = delay + 0.05;
        let (base, should_stop, speed) =
            self.acceleration_from_plan(action_time, stopping_speed)?;
        let lead = if self.mpc.source == MpcSource::Lead1 {
            &input.carrot.radar.lead_two
        } else {
            &input.carrot.radar.lead_one
        };
        let preview_enabled =
            self.mpc.mode == PlannerMode::Acc && !reset && !car.gas_pressed && !car.brake_pressed;
        let request = preview::request(PreviewInput {
            lead_status: preview_enabled && lead.status && lead.radar && lead.radar_track_id >= 0,
            lead_acceleration: lead.a_lead_k,
            ego_acceleration: car.a_ego,
        });
        if preview_enabled {
            let requested = preview::rate_limit(request.offset_s, self.lead_preview);
            self.lead_preview = preview::clip_offset(action_time, requested);
            self.preview_acceleration = request.lead_accel_signal;
            self.preview_action_time = action_time + self.lead_preview;
        } else {
            self.lead_preview = 0.;
            self.preview_acceleration = 0.;
            self.preview_action_time = action_time;
        }
        let (previewed, _, _) =
            self.acceleration_from_plan(self.preview_action_time, stopping_speed)?;
        let acceleration = if preview_enabled {
            preview::apply_target(base, previewed, carrot.driving_mode)
        } else {
            base
        };
        if self.mpc.mode == PlannerMode::Acc {
            self.target_acceleration = acceleration;
            self.target_speed = speed;
            self.should_stop = should_stop;
        } else {
            self.target_acceleration =
                minimum(acceleration, input.carrot.model.action.desired_acceleration);
            self.target_speed = minimum(speed, input.carrot.model.action.desired_velocity);
            self.should_stop = input.carrot.model.action.should_stop || should_stop;
        }
        self.base_acceleration = base;
        self.target_jerk = self.jerk_trajectory[0];
        Ok(())
    }

    pub fn acceleration_from_plan(
        &self,
        time: f64,
        stopping_speed: f64,
    ) -> Result<(f64, bool, f64), Error> {
        let now = self.speed_trajectory[0];
        let acceleration = self.accel_trajectory[0];
        let target = interp(time, &TIMES[..17], &self.speed_trajectory)?;
        let output = 2. * (target - now) / time - acceleration;
        let after = interp(time + 1., &TIMES[..17], &self.speed_trajectory)?;
        Ok((
            output,
            target < stopping_speed && after < stopping_speed,
            now,
        ))
    }
}
