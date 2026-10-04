use super::{CarrotPlanner, DrivingMode, Input, Parameters, PlannerMode, TrafficState, XState};
use crate::{
    driving_mode::TrafficSample,
    following::{self, FollowingInput},
    traffic_stop::ModelLeadInput,
    Error,
};
use openpilot_control_policy::math::{maximum, minimum};

impl CarrotPlanner {
    pub fn update(
        &mut self,
        input: &Input<'_>,
        cruise_kph: f64,
        mode: PlannerMode,
        parameters: &mut impl Parameters,
        clock: &mut impl FnMut() -> f64,
        mono_clock: &mut impl FnMut() -> f64,
    ) -> Result<f64, Error> {
        self.parameters_update(parameters)?;
        self.detector.update(&TrafficSample {
            valid: input.mode_checks,
            dt: 0.05,
            ego_speed: input.car.v_ego,
            lead: input.radar.lead_one,
        });
        if self.config.automatic_mode > 0 && !self.disable_auto {
            self.driving_mode = self.detector.mode(self.config.automatic_mode);
        }
        self.lead_response = self.driving_mode.lead_response(following::response_for_gap(
            self.config.response_base,
            &self.config.response_overrides,
            input.personality,
        ));
        self.update_desire(input)?;
        self.events.clear();
        self.soft_hold_active = input.car.soft_hold_active;
        self.comfort_brake = 2.4 * self.driving_mode.comfort_brake_factor();
        [self.safe_factor, self.follow_factor] = self.driving_mode.factors();
        let cruise_kph = self.eco_control(input.car.v_ego_cluster * 3.6, cruise_kph);
        let (cruise_kph, atc_active) = self.navigation(
            input,
            input.current_navigation(mono_clock),
            cruise_kph,
            clock,
        );
        self.atc_active = atc_active;
        let mut cruise = cruise_kph * (1. / 3.6);
        if input.car.v_clu_ratio > 0.5 {
            cruise *= input.car.v_clu_ratio;
        }
        self.fake_cruise_distance = 0.;
        let position = *input
            .model
            .position
            .x
            .get(31)
            .ok_or(Error::Contract("missing traffic stop horizon"))?;
        self.stop_filter.push(position);
        self.stop_filter2.push(self.stop_filter.median()?);
        self.x_stop = self.stop_filter2.mean()?;
        let stop_raw = self.x_stop;
        let stop_rl = match self.stop_x_rate_limited {
            None => stop_raw,
            Some(previous) if stop_raw > previous => stop_raw,
            Some(previous) => maximum(previous - (input.car.v_ego * 0.05 + 0.5), stop_raw),
        };
        self.stop_x_rate_limited = Some(stop_rl);
        let mut stop_model = stop_rl;
        let previous_traffic = self.traffic_state;
        self.check_stopping(input, cruise)?;
        if self.driving_mode == DrivingMode::High || self.config.traffic_light_mode == 0 {
            self.traffic_state = TrafficState::Off;
        }
        self.transition(
            input,
            &mut cruise,
            [stop_raw, stop_rl],
            &mut stop_model,
            previous_traffic,
            clock,
        )?;
        if matches!(self.traffic_state, TrafficState::Off | TrafficState::Green) || !self.stopping()
        {
            stop_model = 1000.;
        }
        if self.user_stop_distance >= 0. {
            self.user_stop_distance = maximum(0., self.user_stop_distance - input.car.v_ego * 0.05);
            self.actual_stop_distance = self.user_stop_distance;
            self.x_state = if self.user_stop_distance > 0. {
                XState::E2eStop
            } else {
                XState::E2eStopped
            };
        }
        self.mode = if mode == PlannerMode::Acc && self.x_state == XState::E2ePrepare {
            PlannerMode::Blended
        } else {
            mode
        };
        self.actual_stop_distance = maximum(0., self.actual_stop_distance - input.car.v_ego * 0.05);
        if stop_model == 1000. {
            self.actual_stop_distance = 0.;
        } else if self.actual_stop_distance > 0. {
            stop_model = 0.;
        }
        if self.stopping() {
            self.stop_x_rate_limited = Some(stop_raw);
        }
        let stop = maximum(stop_model + self.actual_stop_distance, 0.);
        let lead = if self.stopping() && !input.radar.lead_one.status {
            input.model.leads_v3.first()
        } else {
            None
        };
        let first = |values: &[f64]| values.first().copied().unwrap_or(f64::NAN);
        self.traffic_stop_model_lead_offset = self.matcher.update(&ModelLeadInput {
            stop_active: self.stopping() && stop < 300.,
            allow_confirmation: self.traffic_state == TrafficState::Red && input.car.v_ego > 0.3,
            active_lead: input.radar.lead_one.status,
            stop_distance: stop,
            lead_probability: lead.map_or(f64::NAN, |lead| lead.prob),
            lead_distance: lead.map_or(f64::NAN, |lead| first(&lead.x) - 1.52),
            lead_velocity: lead.map_or(f64::NAN, |lead| first(&lead.v)),
            lead_x_std: lead.map_or(f64::NAN, |lead| first(&lead.x_std)),
            lead_y_std: lead.map_or(f64::NAN, |lead| first(&lead.y_std)),
            lead_v_std: lead.map_or(f64::NAN, |lead| first(&lead.v_std)),
        })?;
        if self.stopping() && stop < 300. {
            let soft = maximum(stop - 1., 0.);
            cruise = minimum(cruise, maximum(0., 2. * self.comfort_brake * soft).sqrt());
        }
        self.cruise_speed = cruise;
        self.stop_distance = stop;
        Ok(cruise_kph)
    }

    pub fn following_time(
        &mut self,
        personality: crate::types::Personality,
        speed: f64,
        acceleration: f64,
    ) -> Result<f64, Error> {
        self.following.update(
            FollowingInput {
                personality,
                speed,
                acceleration,
                mode_factor: self.follow_factor,
            },
            &self.config.gaps,
        )
    }

    pub fn stopping(&self) -> bool {
        matches!(self.x_state, XState::E2eStop | XState::E2eStopped)
    }
}
