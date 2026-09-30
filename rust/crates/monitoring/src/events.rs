use crate::scalar::{max, min};
use crate::{AlertLevel, DriverMonitoring, Input, Policy};
impl DriverMonitoring {
    pub(crate) fn update_events(&mut self, input: &Input) {
        self.alert_level = AlertLevel::None;
        self.driver_interacting = input.steering_pressed || input.gas_pressed;
        if self.alert_3_cnt >= 2 || self.no_response_cnt >= 1 {
            self.too_distracted = true;
        }
        if self.too_distracted {
            self.lockout_time += 1;
            if self.lockout_time > 36000 {
                self.too_distracted = false;
                self.alert_3_cnt = 0;
                self.cnt_since_alert_3 = 0;
                self.no_response_cnt = 0;
                self.lockout_time = 0;
            }
        }
        let always_on_valid = self.always_on && !input.wrong_gear;
        if (self.driver_interacting
            && self.awareness > 0.
            && self.active_policy == Policy::Wheeltouch)
            || (!input.enabled && (!always_on_valid || self.awareness <= 0.))
        {
            self.reset_awareness();
            return;
        }
        let awareness_prev = self.awareness;
        let reaching_alert_1 = self.awareness - self.step_change <= self.threshold_alert_1;
        let reaching_alert_3 = self.awareness - self.step_change <= 0.;
        let lowspeed_exemption = input.car_speed < 2.8 && reaching_alert_1;
        let always_on_exemption = always_on_valid && !input.enabled && reaching_alert_3;
        if self.awareness > 0.
            && ((self.driver_distraction_filter < 0.37 && self.face_detected && self.pose.low_std)
                || lowspeed_exemption)
        {
            if self.driver_interacting {
                self.reset_awareness();
                return;
            }
            self.awareness = min(
                self.awareness + ((5. - 1.25) * (1. - self.awareness) + 1.25) * self.step_change,
                1.,
            );
            if self.awareness == 1. {
                self.last_wheeltouch_awareness =
                    min(self.last_wheeltouch_awareness + self.step_change, 1.);
            }
            if self.awareness > self.threshold_alert_2 {
                return;
            }
        }
        let certainly_distracted =
            self.driver_distraction_filter > 0.63 && self.driver_distracted && self.face_detected;
        let maybe_distracted = self.is_model_uncertain || !self.face_detected;
        if (certainly_distracted || maybe_distracted)
            && !(lowspeed_exemption || always_on_exemption)
        {
            self.awareness = max(self.awareness - self.step_change, -0.1);
        }
        if self.awareness <= 0. {
            self.alert_level = AlertLevel::Three;
            if awareness_prev > 0. {
                self.alert_3_cnt += 1;
                self.cnt_since_alert_3 = 0;
            } else {
                self.cnt_since_alert_3 += 1;
            }
            if self.cnt_since_alert_3 == 100 {
                self.no_response_cnt += 1;
            }
        } else if self.awareness <= self.threshold_alert_2 {
            self.alert_level = AlertLevel::Two;
        } else if self.awareness <= self.threshold_alert_1 {
            self.alert_level = AlertLevel::One;
        }
    }
}
