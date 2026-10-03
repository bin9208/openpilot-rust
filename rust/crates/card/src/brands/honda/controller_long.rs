use super::{can, controller::Controller, state::State, Error, NIDEC_ALT_PCM_ACCEL};
use num_traits::ToPrimitive;
use openpilot_can::Frame;
use openpilot_cereal::car_capnp::{car_control, car_state};
use openpilot_control_policy::math::{clip, interp};

impl Controller {
    pub(super) fn pcm(
        &self,
        state: &State,
        accel: f64,
        gas_brake: [f64; 2],
        active: bool,
    ) -> Result<[f64; 3], Error> {
        let speed = f64::from(
            state
                .out
                .get_root_as_reader::<car_state::Reader>()?
                .get_v_ego(),
        );
        let wind = interp(speed, &[0., 2.3, 35.], &[0.001, 0.002, 0.15])?;
        let max = interp(speed, &[0., 4., 10., 20.], &[0.5, 2.4, 1.4, 0.6])?;
        if !active {
            return Ok([0., 0., wind]);
        }
        let alternative = self.config.static_flags & NIDEC_ALT_PCM_ACCEL != 0;
        let values = [
            0.,
            clip(speed - if alternative { 3. } else { 2. }, 0., 100.),
            clip(speed + if alternative { 0. } else { 2. }, 0., 100.),
            clip(speed + 5., 0., 100.),
        ];
        let pcm_speed = interp(
            gas_brake[0] - gas_brake[1],
            &[-wind, -wind * (3. / 4.), 0., 0.5],
            &values,
        )?;
        let pcm_accel = if alternative {
            198
        } else {
            (clip((accel / 1.44) / max, 0., 1.) * 198.)
                .to_i32()
                .ok_or(Error::Numeric)?
        };
        Ok([pcm_speed, f64::from(pcm_accel), wind])
    }
    pub(super) fn longitudinal(
        &mut self,
        state: &State,
        input: Longitudinal<'_>,
        sends: &mut Vec<Frame>,
    ) -> Result<(), Error> {
        let cc = input.control;
        if !self.config.longitudinal {
            if self.history.frame.is_multiple_of(2) && !self.config.radarless() {
                sends.push(can::send(
                    &mut self.packer,
                    "BOSCH_SUPPLEMENTAL_1",
                    self.config.bus.lkas,
                    &[
                        ("SET_ME_X04", 4.),
                        ("SET_ME_X80", 128.),
                        ("SET_ME_X10", 16.),
                    ],
                )?);
            }
            let cruise = cc.get_cruise_control()?;
            let button = if cruise.get_cancel() {
                Some(2.)
            } else if cruise.get_resume() {
                Some(4.)
            } else {
                None
            };
            if let Some(button) = button {
                sends.push(can::send(
                    &mut self.packer,
                    "SCM_BUTTONS",
                    if self.config.radarless() {
                        self.config.bus.camera
                    } else {
                        self.config.bus.pt
                    },
                    &[("CRUISE_BUTTONS", button), ("CRUISE_SETTING", 0.)],
                )?);
            }
        } else if self.history.frame.is_multiple_of(2) {
            let ts = self.history.frame.to_f64().ok_or(Error::Numeric)? * 0.01;
            if self.config.bosch() {
                self.history.accel = clip(input.accel, -3.5, 2.);
                self.history.gas = interp(input.accel, &[-0.2, 2.], &[0., 1600.])?;
                self.history.stopping_counter = if cc.get_actuators()?.get_long_control_state()?
                    == car_control::actuators::LongControlState::Stopping
                {
                    self.history
                        .stopping_counter
                        .checked_add(1)
                        .ok_or(Error::Numeric)?
                } else {
                    0
                };
                sends.extend(can::acceleration(
                    &mut self.packer,
                    &self.config,
                    can::Acceleration {
                        enabled: cc.get_enabled(),
                        active: cc.get_long_active(),
                        accel: self.history.accel,
                        gas: self.history.gas,
                        stopping_counter: self.history.stopping_counter,
                    },
                )?);
            } else {
                let brake = clip(
                    clip(self.history.brake_last - input.wind, 0., 1.) * 256.,
                    0.,
                    255.,
                )
                .to_i32()
                .ok_or(Error::Numeric)?;
                if brake > self.history.apply_brake_last
                    || (ts - self.history.last_pump_ts > 20. && brake > 0)
                {
                    self.history.last_pump_ts = ts;
                }
                let pump = ts - self.history.last_pump_ts < 0.2 && brake > 0;
                sends.push(can::brake(
                    &mut self.packer,
                    can::Brake {
                        apply: brake,
                        pump,
                        cancel: cc.get_cruise_control()?.get_cancel(),
                        fcw: input.fcw,
                        stock: state
                            .extras
                            .stock_brake
                            .as_ref()
                            .ok_or(Error::Stock("stock_brake"))?,
                        bus: self.config.bus.pt,
                    },
                )?);
                self.history.apply_brake_last = brake;
                self.history.brake = f64::from(brake) / 256.;
            }
        }
        Ok(())
    }
}
pub(super) struct Longitudinal<'a> {
    pub control: car_control::Reader<'a>,
    pub accel: f64,
    pub wind: f64,
    pub fcw: u8,
}
