use super::{CarrotPlanner, EventName, Input, Navigation, XState};
use openpilot_control_policy::math::minimum;

impl CarrotPlanner {
    pub(super) fn navigation(
        &mut self,
        input: &Input<'_>,
        navigation: Option<&Navigation>,
        cruise: f64,
        clock: &mut impl FnMut() -> f64,
    ) -> (f64, bool) {
        let Some(navigation) = navigation else {
            self.traffic_state_carrot = 0;
            self.carrot_stay_stop = false;
            self.active_carrot = 0;
            self.distance_to_turn = 0.;
            self.atc_type.clear();
            return (cruise, false);
        };
        self.carrot_stay_stop = false;
        let trigger = if matches!(navigation.atc_type.as_str(), "turn left" | "atc left")
            || input.car.left_blinker
        {
            if self.traffic_state_carrot == 1 && navigation.traffic_state == 3 {
                true
            } else {
                self.carrot_stay_stop = matches!(navigation.traffic_state, 1 | 2);
                false
            }
        } else {
            self.traffic_state_carrot == 1 && navigation.traffic_state == 2
        };
        self.traffic_state_carrot = navigation.traffic_state;
        if trigger {
            if self.soft_hold_active > 0 {
                self.add_event(EventName::TrafficSignChanged, clock);
            } else if matches!(self.x_state, XState::E2eStop | XState::E2eStopped) {
                self.x_state = XState::E2eCruise;
                self.traffic_starting_count = 200;
            }
        }
        self.active_carrot = navigation.active_carrot;
        self.distance_to_turn = navigation.x_dist_to_turn;
        let active =
            self.active_carrot > 1 && 0. < self.distance_to_turn && self.distance_to_turn < 100.;
        self.atc_type.clone_from(&navigation.atc_type);
        (minimum(cruise, navigation.desired_speed), active)
    }

    pub(super) fn eco_control(&mut self, ego_kph: f64, cruise: f64) -> f64 {
        let mut apply = cruise;
        if self.config.eco_over_speed > 0 {
            if self.eco_target_speed > 0. {
                if self.eco_target_speed < cruise {
                    self.eco_target_speed = cruise;
                } else if self.eco_target_speed > cruise {
                    self.eco_target_speed = 0.;
                }
            } else if self.eco_target_speed == 0. && ego_kph + 3. < cruise && cruise > 20. {
                self.eco_target_speed = cruise;
            }
            if self.eco_target_speed != 0. {
                if ego_kph > self.eco_target_speed {
                    self.eco_target_speed = 0.;
                } else {
                    apply = self.eco_target_speed + f64::from(self.config.eco_over_speed);
                }
            }
        } else {
            self.eco_target_speed = 0.;
        }
        apply
    }
}
