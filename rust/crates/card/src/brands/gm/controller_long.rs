use super::{can, controller::Controller, controller_params, integer, state::State, Error};
use openpilot_can::Frame;
use openpilot_cereal::car_capnp::{
    car_control::{self, actuators::LongControlState},
    car_state,
};
use openpilot_control_policy::math::{interp, minimum};

impl Controller {
    pub(super) fn longitudinal(
        &mut self,
        state: &State,
        cc: car_control::Reader<'_>,
        accel: &mut f64,
        cruise_speed: f64,
        sends: &mut Vec<Frame>,
    ) -> Result<(), Error> {
        let out = state.out.get_root_as_reader::<car_state::Reader>()?;
        let a = cc.get_actuators()?;
        let control_state = a.get_long_control_state()?;
        let stopping =
            control_state == LongControlState::Stopping || out.get_soft_hold_active() > 0;
        let mut brake_accel = f64::from(a.get_accel());
        let orientation = cc.get_orientation_n_e_d()?;
        if self.snapshot.long_pitch && orientation.len() > 1 {
            let alpha = (0.01 * 4.) / (0.09 * 4. + 0.01 * 4.);
            self.snapshot.pitch =
                (1. - alpha) * self.snapshot.pitch + alpha * f64::from(orientation.get(1));
            let pitch = self.snapshot.pitch;
            let deadzone = if pitch > 0.01 {
                pitch - 0.01
            } else if pitch < -0.01 {
                pitch + 0.01
            } else {
                0.
            };
            self.snapshot.accel_g = 9.81 * deadzone;
            *accel += self.snapshot.accel_g;
            brake_accel +=
                self.snapshot.accel_g * interp(f64::from(out.get_v_ego()), &[5., 10.], &[0., 1.])?;
        }
        let near_stop = cc.get_long_active() && f64::from(out.get_v_ego()).abs() < 0.4;
        let mut pedal = 0.;
        let mut regen_paddle = false;
        if !cc.get_long_active() {
            self.snapshot.apply_gas = self.limits.inactive_regen;
            self.snapshot.apply_brake = 0;
        } else if near_stop && stopping && !cc.get_cruise_control()?.get_resume() {
            self.snapshot.apply_gas = self.limits.inactive_regen;
            self.snapshot.apply_brake =
                integer(minimum(-100. * f64::from(self.config.stop_accel), 400.))?;
        } else {
            let ev = self.config.model.ev && self.snapshot.use_ev_tables;
            if ev {
                self.limits.update_ev(f64::from(out.get_v_ego()))?;
            }
            let gas_bp = if ev {
                self.limits.ev_gas_lookup_bp.ok_or(Error::Numeric)?
            } else {
                self.limits.gas_lookup_bp
            };
            let brake_bp = if ev {
                self.limits.ev_brake_lookup_bp.ok_or(Error::Numeric)?
            } else {
                self.limits.brake_lookup_bp
            };
            self.snapshot.apply_gas = integer(
                interp(
                    if self.snapshot.long_pitch {
                        *accel
                    } else {
                        f64::from(a.get_accel())
                    },
                    &gas_bp,
                    &self.limits.gas_lookup_v,
                )?
                .round_ties_even(),
            )?;
            self.snapshot.apply_brake = integer(
                interp(
                    if self.snapshot.long_pitch {
                        brake_accel
                    } else {
                        f64::from(a.get_accel())
                    },
                    &brake_bp,
                    &[400., 0.],
                )?
                .round_ties_even(),
            )?;
            if stopping {
                self.snapshot.apply_gas = self.limits.inactive_regen;
            }
            if self.config.model.cc {
                (pedal, regen_paddle) = controller_params::pedal(
                    f64::from(a.get_accel()),
                    cc.get_long_active(),
                    f64::from(out.get_v_ego()),
                )?;
            }
        }
        if self.config.interceptor
            && self.snapshot.apply_gas > self.limits.inactive_regen
            && out.get_cruise_state()?.get_standstill()
        {
            pedal = 18. / 255.;
            self.snapshot.apply_brake = 0;
            regen_paddle = false;
            self.snapshot.apply_gas = self.limits.inactive_regen;
        }
        let counter = i32::try_from((self.snapshot.frame / 4) % 4).map_err(|_| Error::Numeric)?;
        if self.config.flags & 2 != 0
            && cc.get_long_active()
            && out.get_v_ego() > self.config.min_enable
        {
            self.spam(state, f64::from(a.get_accel()), sends)?;
        }
        if self.config.interceptor {
            sends.push(can::pedal(&mut self.pt, pedal, counter)?);
            if self.config.model.name == "CHEVROLET_BOLT_CC" && regen_paddle {
                sends.push(can::message(
                    &mut self.pt,
                    "EBCMRegenPaddle",
                    0,
                    &[("RegenPaddle", 32.)],
                )?);
            }
        }
        if !self.config.model.cc {
            let mut full_stop = cc.get_long_active() && out.get_standstill();
            let bus = if self.config.camera {
                full_stop &= stopping;
                0
            } else {
                2
            };
            if self.config.auto_resume {
                let resume = control_state != LongControlState::Starting
                    || cc.get_cruise_control()?.get_resume();
                full_stop &= !resume;
            }
            let engaged = if cc.get_cruise_control()?.get_resume()
                && state
                    .extras
                    .pcm_acc_status
                    .ok_or(Error::Stock("pcm_acc_status"))?
                    == 4.
            {
                false
            } else {
                cc.get_enabled()
            };
            if matches!(
                control_state,
                LongControlState::Stopping | LongControlState::Starting
            ) && self.button_ready()?
            {
                self.snapshot.last_button_frame = self.snapshot.frame;
                sends.push(can::buttons(
                    &mut self.pt,
                    0,
                    (integer(state.extras.buttons_counter)? + 1).rem_euclid(4),
                    2,
                )?);
            }
            sends.push(can::gas(
                &mut self.pt,
                self.snapshot.apply_gas,
                counter,
                engaged,
                full_stop,
            )?);
            sends.push(can::brake(
                &mut self.chassis,
                bus,
                self.snapshot.apply_brake,
                counter,
                cc.get_enabled() && self.config.model.bolt(),
                full_stop,
                false,
            )?);
            sends.push(can::dashboard(
                &mut self.pt,
                cc.get_enabled(),
                cruise_speed * 3.6,
                cc.get_hud_control()?,
            )?);
        }
        Ok(())
    }
}
