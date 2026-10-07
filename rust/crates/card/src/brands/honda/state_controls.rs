use super::{
    state::{copy, float, State, Stock},
    Error, BOSCH_ALT_BRAKE,
};
use openpilot_cereal::car_capnp::car_state;

impl State {
    pub(super) fn controls(
        &mut self,
        ret: &mut car_state::Builder<'_>,
        now: u64,
    ) -> Result<(), Error> {
        let mut cruise = ret.reborrow().init_cruise_state();
        if self.config.bosch() {
            if self.config.radarless() {
                cruise.set_non_adaptive(
                    self.camera_signal("ACC_HUD", "CRUISE_CONTROL_LABEL", now)? != 0.,
                );
            }
            if !self.config.longitudinal {
                let parser = if self.config.radarless() {
                    &mut self.camera
                } else {
                    &mut self.pt
                };
                let nonadaptive = parser.signal_lazy("ACC_HUD", "CRUISE_CONTROL_LABEL", now)? != 0.;
                let speed = parser.signal_lazy("ACC_HUD", "CRUISE_SPEED", now)?;
                cruise.set_non_adaptive(nonadaptive);
                cruise.set_standstill(speed == 252.);
                let speed = float(if speed > 160. {
                    self.extras.v_cruise_pcm_prev
                } else {
                    speed * self.config.conversion(self.is_metric)
                })?;
                cruise.set_speed(speed);
                self.extras.v_cruise_pcm_prev = f64::from(speed);
            }
        } else {
            cruise.set_speed(float(
                self.signal("CRUISE", "CRUISE_SPEED_PCM", now)? * (1. / 3.6),
            )?);
        }
        if self.config.flags & BOSCH_ALT_BRAKE != 0 {
            ret.set_brake_pressed(self.signal("BRAKE_MODULE", "BRAKE_PRESSED", now)? != 0.);
        } else {
            let address = self.pt.dbc.message("POWERTRAIN_DATA")?.address;
            let index = self
                .pt
                .dbc
                .message("POWERTRAIN_DATA")?
                .signals
                .iter()
                .position(|s| s.name == "BRAKE_SWITCH")
                .ok_or_else(|| Error::Signal("BRAKE_SWITCH".into()))?;
            let values = self
                .pt
                .states
                .get(&address)
                .and_then(|s| s.all_values.get(index))
                .ok_or_else(|| Error::Signal("BRAKE_SWITCH".into()))?;
            if !values.is_empty() {
                if values.len() > 1 {
                    self.extras.brake_switch_prev = values[values.len() - 2] != 0.;
                }
                let brake = self.signal("POWERTRAIN_DATA", "BRAKE_SWITCH", now)? != 0.;
                self.extras.brake_switch_active = brake && self.extras.brake_switch_prev;
                self.extras.brake_switch_prev = brake;
            }
            ret.set_brake_pressed(
                self.signal("POWERTRAIN_DATA", "BRAKE_PRESSED", now)? != 0.
                    || self.extras.brake_switch_active,
            );
        }
        let brake = float(self.signal("VSA_STATUS", "USER_BRAKE", now)?)?;
        ret.set_brake(brake);
        let enabled = self.signal("POWERTRAIN_DATA", "ACC_STATUS", now)? != 0.;
        let available = self.signal(self.main_message, "MAIN_ON", now)? != 0.;
        ret.reborrow().get_cruise_state()?.set_enabled(enabled);
        ret.reborrow().get_cruise_state()?.set_available(available);
        if matches!(
            self.config.candidate.as_str(),
            "HONDA_PILOT" | "HONDA_RIDGELINE"
        ) && brake > 0.1
        {
            ret.set_brake_pressed(true);
        }
        if self.config.bosch() {
            if !self.config.radarless() {
                ret.set_stock_aeb(
                    !self.config.longitudinal
                        && self.signal("ACC_CONTROL", "AEB_STATUS", now)? != 0.
                        && self.signal("ACC_CONTROL", "ACCEL_COMMAND", now)? < -1e-5,
                );
            }
        } else {
            ret.set_stock_aeb(
                self.camera_signal("BRAKE_COMMAND", "AEB_REQ_1", now)? != 0.
                    && self.camera_signal("BRAKE_COMMAND", "COMPUTER_BRAKE", now)? > 1e-5,
            );
        }
        self.extras.acc_hud = Some(Stock::Inactive(false));
        self.extras.lkas_hud = Some(Stock::Inactive(false));
        if !self.config.bosch() {
            ret.set_stock_fcw(self.camera_signal("BRAKE_COMMAND", "FCW", now)? != 0.);
            self.extras.acc_hud = Some(Stock::Values(copy(&mut self.camera, "ACC_HUD", now)?));
            self.extras.stock_brake = Some(copy(&mut self.camera, "BRAKE_COMMAND", now)?);
        }
        if self.config.radarless() {
            self.extras.lkas_hud = Some(Stock::Values(copy(&mut self.camera, "LKAS_HUD", now)?));
        }
        if let Some(body) = &mut self.body {
            ret.set_left_blindspot(body.signal_lazy("BSM_STATUS_LEFT", "BSM_ALERT", now)? == 1.);
            ret.set_right_blindspot(body.signal_lazy("BSM_STATUS_RIGHT", "BSM_ALERT", now)? == 1.);
        }
        Ok(())
    }
}
