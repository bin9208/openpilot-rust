use super::{
    state::State, state_motion::float, Error, DISABLE_RADAR, NO_STOP_TIMER, TSS2, UNSUPPORTED_DSU,
};
use openpilot_cereal::car_capnp::car_state::{self, button_event::Type as Button};

impl State {
    pub(super) fn cruise(
        &mut self,
        ret: &mut car_state::Builder<'_>,
        now: u64,
    ) -> Result<(), Error> {
        let unsupported = self.config.static_flags & UNSUPPORTED_DSU != 0;
        let cruise_name = if unsupported {
            "DSU_CRUISE"
        } else {
            "PCM_CRUISE_2"
        };
        if !unsupported {
            ret.set_acc_faulted(self.signal(false, "PCM_CRUISE_2", "ACC_FAULTED", now)? != 0.);
        }
        let available = self.signal(false, cruise_name, "MAIN_ON", now)? != 0.;
        let speed = float(self.signal(false, cruise_name, "SET_SPEED", now)? * (1. / 3.6))?;
        let cluster_name = if unsupported {
            "PCM_CRUISE_ALT"
        } else {
            "PCM_CRUISE_SM"
        };
        let cluster_set = self.signal(false, cluster_name, "UI_SET_SPEED", now)?;
        let mut cruise = ret.reborrow().init_cruise_state();
        cruise.set_available(available);
        cruise.set_speed(speed);
        if speed != 0. {
            let metric =
                [1., 2.].contains(&self.signal(false, "BODY_CONTROL_STATE_2", "UNITS", now)?);
            cruise.set_speed_cluster(float(
                cluster_set * if metric { 1. / 3.6 } else { 0.44704 },
            )?);
        }
        let tss2 = self.config.static_flags & TSS2 != 0;
        if tss2 && self.config.flags & DISABLE_RADAR == 0 {
            self.extras.acc_type =
                self.signal(self.acc_camera(), "ACC_CONTROL", "ACC_TYPE", now)?;
            ret.set_stock_fcw(self.signal(self.acc_camera(), "PCS_HUD", "FCW", now)? != 0.);
        }
        if ((!tss2 && !unsupported) || (tss2 && self.extras.acc_type == 1.))
            && self.config.longitudinal
            && !ret.reborrow_as_reader().get_acc_faulted()
        {
            ret.set_acc_faulted(
                self.signal(false, "PCM_CRUISE_2", "LOW_SPEED_LOCKOUT", now)? == 2.,
            );
        }
        let status = self.signal(false, "PCM_CRUISE", "CRUISE_STATE", now)?;
        self.extras.pcm_acc_status = Some(status);
        let mut cruise = ret.reborrow().get_cruise_state()?;
        if self.config.static_flags & NO_STOP_TIMER == 0 || tss2 {
            cruise.set_standstill(status == 7.);
        }
        cruise.set_enabled(self.signal(false, "PCM_CRUISE", "CRUISE_ACTIVE", now)? != 0.);
        cruise.set_non_adaptive([1., 2., 3., 4., 5., 6.].contains(&status));
        ret.set_generic_toggle(self.signal(false, "LIGHT_STALK", "AUTO_HIGH_BEAM", now)? != 0.);
        ret.set_esp_disabled(self.signal(false, "ESP_CONTROL", "TC_DISABLED", now)? != 0.);
        if self.config.bsm {
            ret.set_left_blindspot(
                self.signal(false, "BSM", "L_ADJACENT", now)? == 1.
                    || self.signal(false, "BSM", "L_APPROACHING", now)? == 1.,
            );
            ret.set_right_blindspot(
                self.signal(false, "BSM", "R_ADJACENT", now)? == 1.
                    || self.signal(false, "BSM", "R_APPROACHING", now)? == 1.,
            );
        }
        if self.config.candidate != "TOYOTA_PRIUS_V" {
            self.extras.lkas_hud = self.copied(true, "LKAS_HUD", now)?;
        }
        if !unsupported {
            self.extras.pcm_follow_distance =
                self.signal(false, "PCM_CRUISE_2", "PCM_FOLLOW_DISTANCE", now)?;
        }
        if self.acc_camera() {
            ret.reborrow().init_button_events(0);
            let previous = self.extras.distance_button;
            let current = self.signal(true, "ACC_CONTROL", "DISTANCE", now)?;
            self.extras.distance_button = current;
            if current != previous {
                let changes = [(previous, false), (current, true)]
                    .into_iter()
                    .filter(|(value, _)| *value != 0.)
                    .collect::<Vec<_>>();
                let mut buttons = ret
                    .reborrow()
                    .init_button_events(u32::try_from(changes.len()).map_err(|_| Error::Numeric)?);
                for (index, (value, pressed)) in changes.into_iter().enumerate() {
                    let mut button = buttons
                        .reborrow()
                        .get(u32::try_from(index).map_err(|_| Error::Numeric)?);
                    button.set_type(if value == 1. {
                        Button::GapAdjustCruise
                    } else {
                        Button::Unknown
                    });
                    button.set_pressed(pressed);
                }
            }
        }
        Ok(())
    }
}
