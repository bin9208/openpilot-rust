use super::{CarrotPlanner, Input, LongitudinalPlanner};
use crate::{
    carrot::XState,
    coasting::CoastingInput,
    types::{MpcSource, PlannerMode},
};

pub(super) struct Conditions {
    pub cruise_kph: f64,
    pub cruise: f64,
    pub reset: bool,
    pub turn_blocked: bool,
    pub cutin: Option<f64>,
}

impl LongitudinalPlanner {
    pub(super) fn update_coasting(
        &mut self,
        input: &Input<'_>,
        carrot: &CarrotPlanner,
        conditions: Conditions,
        clock: &mut impl FnMut() -> f64,
    ) {
        self.coasting_target = 0.;
        if self.coasting_percent <= 0 {
            self.coasting.reset();
            return;
        }
        let car = input.carrot.car;
        let radar = input.carrot.radar;
        let navigation = input.carrot.current_navigation(clock);
        let current = !input.navigation_seen || navigation.is_some();
        let ratio = if car.v_clu_ratio > 0.5 {
            car.v_clu_ratio
        } else {
            1.
        };
        let external = navigation.map_or(250. * (1. / 3.6), |navigation| {
            navigation.desired_speed * (1. / 3.6) * ratio
        });
        let no_lead =
            !radar.lead_one.status && !radar.lead_two.status && !radar.lead_cut_in_risk.status;
        let enabled = self.vehicle.longitudinal_control
            && !conditions.reset
            && self.reset_decel_timer == 0
            && input.coasting_checks
            && self.mpc.mode == PlannerMode::Acc
            && self.mpc.source == MpcSource::Cruise
            && self.mpc.solution_status == 0
            && current
            && !input.force_deceleration
            && !self.should_stop
            && !self.fcw
            && !car.gas_pressed
            && !car.brake_pressed
            && car.carrot_cruise == 0
            && !car.standstill
            && carrot.soft_hold_active == 0
            && no_lead
            && conditions.cutin.is_none()
            && !conditions.turn_blocked
            && !carrot.atc_active
            && !carrot.lane_change_active
            && matches!(carrot.x_state, XState::Cruise | XState::E2eCruise)
            && carrot.eco_target_speed == 0.
            && (self.cruise_kph - conditions.cruise_kph).abs() < 0.001
            && (carrot.cruise_speed - conditions.cruise).abs() < 0.001;
        self.coasting_target = self.coasting.update(&CoastingInput {
            enabled,
            percent: f64::from(self.coasting_percent),
            set_speed: conditions.cruise_kph,
            target: conditions.cruise,
            external_limit: external,
            dt: self.dt,
        });
    }
}
