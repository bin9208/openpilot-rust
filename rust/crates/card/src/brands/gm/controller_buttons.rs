use super::{can, controller::Controller, integer, state::State, Error};
use num_traits::ToPrimitive;
use openpilot_can::Frame;
use openpilot_cereal::car_capnp::{
    car_control::{self, actuators::LongControlState},
    car_state,
};
use openpilot_control_policy::math::maximum;

impl Controller {
    pub(super) fn auto_resume(
        &mut self,
        state: &State,
        cc: car_control::Reader<'_>,
        sends: &mut Vec<Frame>,
    ) -> Result<(), Error> {
        let out = state.out.get_root_as_reader::<car_state::Reader>()?;
        if self.config.model.volt() {
            if out.get_activate_cruise() != 0 && !out.get_cruise_state()?.get_enabled() {
                self.snapshot.activate_cruise_after_brake = false;
                if self.button_ready()? {
                    self.snapshot.last_button_frame = self.snapshot.frame;
                    sends.push(can::buttons(
                        &mut self.pt,
                        0,
                        (integer(state.extras.buttons_counter)? + 1).rem_euclid(4),
                        3,
                    )?);
                }
            } else if cc.get_actuators()?.get_long_control_state()? == LongControlState::Starting
                && out.get_cruise_state()?.get_enabled()
                && !self.snapshot.activate_cruise_after_brake
            {
                let counter =
                    i32::try_from((self.snapshot.frame / 4) % 4).map_err(|_| Error::Numeric)?;
                sends.push(can::brake(
                    &mut self.chassis,
                    2,
                    -50,
                    counter,
                    false,
                    false,
                    true,
                )?);
                self.writes
                    .push(("ActivateCruiseAfterBrake".into(), b"1".to_vec()));
                self.snapshot.activate_cruise_after_brake = true;
            }
        } else if out.get_activate_cruise() != 0
            && !out.get_cruise_state()?.get_enabled()
            && self.button_ready()?
        {
            self.snapshot.last_button_frame = self.snapshot.frame;
            sends.push(can::buttons(
                &mut self.pt,
                0,
                (integer(state.extras.buttons_counter)? + 1).rem_euclid(4),
                3,
            )?);
        }
        Ok(())
    }
    pub(super) fn spam(
        &mut self,
        state: &State,
        accel: f64,
        sends: &mut Vec<Frame>,
    ) -> Result<(), Error> {
        let metric = self.settings.get_bool("IsMetric")?;
        let conversion = if metric {
            3.6
        } else {
            1. / (1.609344 * (1. / 3.6))
        };
        let max_rate = if metric { 0.04 } else { 0.2 };
        let out = state.out.get_root_as_reader::<car_state::Reader>()?;
        let accel = accel * conversion;
        let target = integer(
            (f64::from(out.get_cruise_state()?.get_speed()) * conversion).round_ties_even(),
        )?;
        let (button, rate) =
            if f64::from(target) == f64::from(self.config.min_enable) && accel < -1. {
                self.snapshot.apply_speed = 0;
                (6, 0.04)
            } else if accel < 0. {
                self.snapshot.apply_speed = target.checked_sub(1).ok_or(Error::Numeric)?;
                (
                    3,
                    if f64::from(target) > f64::from(out.get_v_ego()) * conversion + 3. {
                        max_rate
                    } else {
                        maximum(-1. / accel, max_rate)
                    },
                )
            } else if accel > 0. {
                self.snapshot.apply_speed = target.checked_add(1).ok_or(Error::Numeric)?;
                (
                    2,
                    if f64::from(target) < f64::from(out.get_v_ego()) * conversion - 3. {
                        max_rate
                    } else {
                        maximum(1. / accel, max_rate)
                    },
                )
            } else {
                self.snapshot.apply_speed = target;
                (0, f64::INFINITY)
            };
        let elapsed = (self.snapshot.frame - self.snapshot.last_button_frame)
            .to_f64()
            .ok_or(Error::Numeric)?
            * 0.01;
        if button != 0 && elapsed > rate {
            self.snapshot.last_button_frame = self.snapshot.frame;
            sends.push(can::buttons(
                &mut self.pt,
                0,
                (integer(state.extras.buttons_counter)? + 1).rem_euclid(4),
                button,
            )?);
        }
        Ok(())
    }
}
