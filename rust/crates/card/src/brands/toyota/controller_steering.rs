use super::{can, controller::Controller, secoc::Freshness, state::State, Error, SECOC, TSS2};
use crate::{core::VehicleLog, query::DiagnosticLevel};
use num_traits::ToPrimitive;
use openpilot_can::Frame;
use openpilot_cereal::car_capnp::{car_control, car_state};
use openpilot_control_policy::math::{clip, interp, maximum, minimum};

impl Controller {
    fn freshness(&self, state: &State, message: u64) -> Result<Freshness, Error> {
        let values = state
            .extras
            .secoc_synchronization
            .as_ref()
            .ok_or(Error::Stock("secoc_synchronization"))?;
        let trip = values
            .get("TRIP_CNT")
            .ok_or_else(|| Error::Signal("TRIP_CNT".into()))?
            .to_u16()
            .ok_or(Error::Numeric)?;
        let reset = values
            .get("RESET_CNT")
            .ok_or_else(|| Error::Signal("RESET_CNT".into()))?
            .to_u32()
            .ok_or(Error::Numeric)?;
        Ok(Freshness {
            trip,
            reset,
            message,
        })
    }
    fn synchronization(&mut self, state: &State) -> Result<(), Error> {
        let values = state
            .extras
            .secoc_synchronization
            .as_ref()
            .ok_or(Error::Stock("secoc_synchronization"))?;
        let reset = values
            .get("RESET_CNT")
            .copied()
            .ok_or_else(|| Error::Signal("RESET_CNT".into()))?;
        if reset != self.history.secoc_prev_reset_counter {
            self.history.secoc_lka_message_counter = 0;
            self.history.secoc_lta_message_counter = 0;
            self.history.secoc_prev_reset_counter = reset;
            let freshness = self.freshness(state, 0)?;
            let expected = self.key.sync_mac(freshness.trip, freshness.reset)?;
            let actual = values
                .get("AUTHENTICATOR")
                .ok_or_else(|| Error::Signal("AUTHENTICATOR".into()))?
                .to_u32()
                .ok_or(Error::Numeric)?;
            if actual != expected {
                self.logs.push(VehicleLog {
                    level: DiagnosticLevel::Error,
                    message: "SecOC synchronization MAC mismatch, wrong key?".into(),
                });
            }
        }
        Ok(())
    }
    pub(super) fn steering(
        &mut self,
        state: &State,
        cc: car_control::Reader<'_>,
    ) -> Result<Vec<Frame>, Error> {
        let out = state.out.get_root_as_reader::<car_state::Reader>()?;
        let actuators = cc.get_actuators()?;
        let lat_active = cc.get_lat_active() && out.get_steering_torque().abs() < 500.;
        if self.config.flags & SECOC != 0 {
            self.synchronization(state)?;
        }
        let maximum_torque = f64::from(self.maximum);
        let command = (f64::from(actuators.get_torque()) * maximum_torque).round_ties_even();
        if !command.is_finite() {
            return Err(Error::Numeric);
        }
        let measured = f64::from(out.get_steering_torque_eps());
        let upper = minimum(maximum(measured + 350., 350.), maximum_torque);
        let lower = maximum(minimum(measured - 350., -350.), -maximum_torque);
        let limited = clip(command, lower, upper);
        let previous = f64::from(self.history.last_torque);
        let up = f64::from(self.delta_up);
        let down = f64::from(self.delta_down);
        let limited = if previous > 0. {
            clip(limited, maximum(previous - down, -up), previous + up)
        } else {
            clip(limited, previous - up, minimum(previous + down, up))
        };
        let mut torque = limited.round_ties_even().to_i32().ok_or(Error::Numeric)?;
        self.history.steer_rate_counter = if out.get_steering_rate_deg().abs() >= 100. && lat_active
        {
            self.history
                .steer_rate_counter
                .checked_add(1)
                .ok_or(Error::Numeric)?
        } else {
            0
        };
        let mut request = lat_active && self.history.steer_rate_counter <= 18;
        if self.history.steer_rate_counter >= 19 {
            self.history.steer_rate_counter = 0;
        }
        if !lat_active {
            torque = 0;
        }
        if self.config.angle {
            torque = 0;
            request = false;
            if self.history.frame.is_multiple_of(2) {
                let target = f64::from(actuators.get_steering_angle_deg())
                    + f64::from(out.get_steering_angle_offset_deg());
                let increasing = self.history.last_angle * target >= 0.
                    && target.abs() > self.history.last_angle.abs();
                let limit = interp(
                    f64::from(out.get_v_ego_raw()),
                    &[5., 25.],
                    if increasing {
                        &[0.3, 0.15]
                    } else {
                        &[0.36, 0.26]
                    },
                )?;
                let angle = if cc.get_lat_active() {
                    clip(
                        target,
                        self.history.last_angle - limit,
                        self.history.last_angle + limit,
                    )
                } else {
                    f64::from(out.get_steering_angle_deg())
                        + f64::from(out.get_steering_angle_offset_deg())
                };
                self.history.last_angle = clip(angle, -94.9461, 94.9461);
            }
        }
        self.history.last_torque = torque;
        let mut steering = can::send(
            &mut self.packer,
            "STEERING_LKA",
            &[
                ("STEER_REQUEST", f64::from(request)),
                ("STEER_TORQUE_CMD", f64::from(torque)),
                ("SET_ME_1", 1.),
            ],
        )?;
        if self.config.flags & SECOC != 0 {
            steering = self.key.authenticate(
                self.freshness(state, self.history.secoc_lka_message_counter)?,
                steering,
            )?;
            self.history.secoc_lka_message_counter = self
                .history
                .secoc_lka_message_counter
                .checked_add(1)
                .ok_or(Error::Numeric)?;
        }
        let mut sends = vec![steering];
        if self.history.frame.is_multiple_of(2) && self.config.static_flags & TSS2 != 0 {
            let active = lat_active && self.config.angle;
            let full = f64::from(out.get_steering_torque_eps().abs()) < f64::from(self.maximum)
                && out.get_steering_torque().abs() < 150.;
            let counter = (self.history.frame / 2)
                .checked_add(128)
                .ok_or(Error::Numeric)?
                .to_f64()
                .ok_or(Error::Numeric)?;
            sends.push(can::send(
                &mut self.packer,
                "STEERING_LTA",
                &[
                    ("COUNTER", counter),
                    ("SETME_X1", 1.),
                    ("SETME_X3", if self.config.angle { 1. } else { 3. }),
                    ("PERCENTAGE", 100.),
                    ("TORQUE_WIND_DOWN", if active && full { 100. } else { 0. }),
                    ("ANGLE", 0.),
                    ("STEER_ANGLE_CMD", self.history.last_angle),
                    ("STEER_REQUEST", f64::from(active)),
                    ("STEER_REQUEST_2", f64::from(active)),
                    ("CLEAR_HOLD_STEERING_ALERT", 0.),
                ],
            )?);
            if self.config.flags & SECOC != 0 {
                let frame = can::send(&mut self.packer, "STEERING_LTA_2", &[("COUNTER", counter)])?;
                sends.push(self.key.authenticate(
                    self.freshness(state, self.history.secoc_lta_message_counter)?,
                    frame,
                )?);
                self.history.secoc_lta_message_counter = self
                    .history
                    .secoc_lta_message_counter
                    .checked_add(1)
                    .ok_or(Error::Numeric)?;
            }
        }
        Ok(sends)
    }
}
