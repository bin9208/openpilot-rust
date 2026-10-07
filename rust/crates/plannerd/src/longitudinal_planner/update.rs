use super::{CarrotPlanner, Input, LongControlState, LongitudinalPlanner, Parameters};
use crate::{
    lead_obstacles, longitudinal_mpc,
    turn_accel::{self, TurnInput},
    types::PlannerMode,
    Error,
};
use num_traits::ToPrimitive;
use openpilot_control_policy::math::{clip, maximum, minimum};

impl LongitudinalPlanner {
    pub fn update(
        &mut self,
        input: &Input<'_>,
        carrot: &mut CarrotPlanner,
        parameters: &mut impl Parameters,
        wall_clock: &mut impl FnMut() -> f64,
        mono_clock: &mut impl FnMut() -> f64,
    ) -> Result<Option<i32>, Error> {
        self.coasting_param_time += self.dt;
        if self.coasting_param_time >= 1. {
            self.coasting_param_time = 0.;
            self.coasting_percent = parameters.integer("CruiseCoastingPercent")?.clamp(0, 10);
        }
        self.mpc.mode = if input.experimental {
            PlannerMode::Blended
        } else {
            PlannerMode::Acc
        };
        let car = input.carrot.car;
        let speed = car.v_ego;
        let cruise_kph = minimum(car.v_cruise, 145.);
        self.cruise_kph = carrot.update(
            &input.carrot,
            cruise_kph,
            self.mpc.mode,
            parameters,
            wall_clock,
            mono_clock,
        )?;
        self.mpc.mode = carrot.mode;
        let mut cruise = self.cruise_kph * (1. / 3.6);
        if car.v_clu_ratio > 0.5 {
            self.cluster_ratio = car.v_clu_ratio;
            cruise *= car.v_clu_ratio;
        }
        let reset = (if self.vehicle.longitudinal_control {
            input.control_state == LongControlState::Off
        } else {
            !input.enabled
        }) || car.v_cruise == 255.
            || carrot.soft_hold_active != 0;
        let previous_constraint = !(reset || car.standstill);
        let (limits, mut turn_limits) = if self.mpc.mode == PlannerMode::Acc {
            let limits = [-2., carrot.acceleration(speed)?];
            let future =
                turn_accel::future_curvature(input.carrot.model, input.desired_curvature, 1.)?;
            let turns = turn_accel::limit(
                &TurnInput {
                    speed,
                    curvature: future,
                    acceleration: limits,
                    lateral_maximum: 3.,
                    safety_ratio: 0.70,
                    minimum_speed: 0.1,
                    cruise_speed: Some(cruise),
                    current_curvature: input.curvature,
                },
                Some(input.carrot.model),
            )?;
            (limits, turns)
        } else {
            ([-4., 2.5], [-4., 2.5])
        };
        let turn_blocked = turn_limits[1] < limits[1] - 0.05;
        if reset {
            self.set_speed_filter(speed);
            self.desired_acceleration = clip(car.a_ego, limits[0], limits[1]);
            self.mpc
                .previous_acceleration
                .fill(self.desired_acceleration);
            self.reset_decel_timer = (2. / self.dt)
                .to_u64()
                .ok_or(Error::Contract("reset ramp duration"))?;
            self.reset_decel_start = self.desired_acceleration;
        } else if self.reset_decel_timer > 0 {
            let steps = (2. / self.dt)
                .to_u64()
                .ok_or(Error::Contract("reset ramp duration"))?
                .max(1);
            let time = clip(
                self.reset_decel_timer
                    .to_f64()
                    .ok_or(Error::Contract("reset ramp counter"))?
                    / steps.to_f64().ok_or(Error::Contract("reset ramp steps"))?,
                0.,
                1.,
            );
            self.reset_decel_timer -= 1;
            let soft = minimum(0., self.reset_decel_start - 0.05);
            let ramped = soft * time + turn_limits[0] * (1. - time);
            turn_limits[0] = maximum(turn_limits[0], minimum(0., ramped));
        }
        let filtered = self.desired_speed.update(speed);
        self.set_speed_filter(maximum(0., filtered));
        let reference = Self::parse_model(input.carrot.model)?;
        if input.force_deceleration {
            cruise = 0.;
        }
        let cutin = if !reset && !car.gas_pressed {
            lead_obstacles::predecel_limit(&input.carrot.radar.lead_cut_in_risk)
        } else {
            None
        };
        turn_limits[0] = minimum(turn_limits[0], self.desired_acceleration + 0.05);
        turn_limits[1] =
            lead_obstacles::apply_predecel(turn_limits[1], self.desired_acceleration, cutin);
        self.update_tracks(input.carrot.radar);
        let response_enabled = carrot.lead_response > 0
            && !carrot.lane_change_active
            && !reset
            && !car.gas_pressed
            && !input.force_deceleration
            && turn_limits[1] > 0.
            && !self.should_stop;
        self.mpc.acceleration_limits(turn_limits[0], turn_limits[1]);
        self.mpc
            .current_state(self.desired_speed.value(), self.desired_acceleration)?;
        let change_cost_starting = carrot.config.change_cost_starting;
        let gap_enabled =
            !reset && !car.gas_pressed && !input.force_deceleration && !carrot.lane_change_active;
        let warning = self.mpc.update(
            longitudinal_mpc::Input {
                carrot,
                radar: input.carrot.radar,
                reset,
                cruise,
                reference,
                personality: input.carrot.personality,
                previous_accel_constraint: previous_constraint,
                change_cost_starting,
                response_enabled,
                gap_enabled,
                track_frames: self.track_frames,
                measured_acceleration: car.a_ego,
                cutout_enabled: !reset
                    && !car.gas_pressed
                    && !input.force_deceleration
                    && !self.should_stop,
            },
            mono_clock,
        )?;
        self.targets(input, carrot, reset, parameters)?;
        self.update_coasting(
            input,
            carrot,
            super::coasting::Conditions {
                cruise_kph,
                cruise,
                reset,
                turn_blocked,
                cutin,
            },
            mono_clock,
        );
        Ok(warning)
    }
}
