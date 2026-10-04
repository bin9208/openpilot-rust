use super::{can, state::State, Error, GLOBAL_GEN2, PREGLOBAL, STEER_RATE_LIMITED};
use crate::core::{ApplyInput, ApplyOutput, Message, VehicleLog};
use num_traits::ToPrimitive;
use openpilot_can::{packer::Packer, Frame};
use openpilot_cereal::car_capnp::{
    car_control::{self, actuators},
    car_params, car_state,
};
use openpilot_control_policy::math::clip;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Default, Serialize)]
pub struct Snapshot {
    pub frame: u64,
    pub apply_torque_last: i32,
    pub cruise_button_prev: f64,
    pub steer_rate_counter: u32,
}

pub struct Controller {
    pub snapshot: Snapshot,
    pub logs: Vec<VehicleLog>,
    pub(super) packer: Packer,
    pub(super) flags: u32,
    pub(super) longitudinal: bool,
    maximum: i32,
    delta_up: f64,
    delta_down: f64,
}
impl Controller {
    pub fn new(packer: Packer, cp: car_params::Reader<'_>) -> Result<Self, Error> {
        let flags = cp.get_flags();
        let (maximum, delta_up, delta_down) = if flags & GLOBAL_GEN2 != 0 {
            (1000, 40., 40.)
        } else if cp.get_car_fingerprint()?.to_str()? == "SUBARU_IMPREZA_2020" {
            (1439, 35., 70.)
        } else {
            (2047, 50., 70.)
        };
        Ok(Self {
            snapshot: Snapshot::default(),
            logs: Vec::new(),
            packer,
            flags,
            longitudinal: cp.get_openpilot_longitudinal_control(),
            maximum,
            delta_up,
            delta_down,
        })
    }
    pub fn packer_counters(&self) -> BTreeMap<u32, String> {
        self.packer
            .counters
            .iter()
            .map(|(address, value)| (*address, value.to_string()))
            .collect()
    }
    fn limited(&self, command: f64, driver: f32) -> Result<i32, Error> {
        if !command.is_finite() {
            return Err(Error::Numeric);
        }
        let maximum = f64::from(self.maximum);
        let upper = maximum
            .min(maximum + (60. + f64::from(driver)) * 50.)
            .max(0.);
        let lower = (-maximum)
            .max(-maximum + (-60. + f64::from(driver)) * 50.)
            .min(0.);
        let command = clip(command, lower, upper);
        let previous = f64::from(self.snapshot.apply_torque_last);
        let command = if previous > 0. {
            clip(
                command,
                (previous - self.delta_down).max(-self.delta_up),
                previous + self.delta_up,
            )
        } else {
            clip(
                command,
                previous - self.delta_up,
                (previous + self.delta_down).min(self.delta_up),
            )
        };
        command.round_ties_even().to_i32().ok_or(Error::Numeric)
    }
    fn steering(&mut self, state: &State, cc: car_control::Reader<'_>) -> Result<Frame, Error> {
        let out = state.out.get_root_as_reader::<car_state::Reader>()?;
        let mut torque = self.limited(
            (f64::from(cc.get_actuators()?.get_torque()) * f64::from(self.maximum))
                .round_ties_even(),
            out.get_steering_torque(),
        )?;
        if !cc.get_lat_active() {
            torque = 0;
        }
        let sent = if self.flags & PREGLOBAL != 0 {
            can::preglobal(
                &mut self.packer,
                "ES_LKAS",
                vec![
                    (
                        "COUNTER",
                        (self.snapshot.frame / 2 % 8)
                            .to_f64()
                            .ok_or(Error::Numeric)?,
                    ),
                    ("LKAS_Command", f64::from(torque)),
                    ("LKAS_Active", f64::from(cc.get_lat_active())),
                ],
            )?
        } else {
            let mut request = cc.get_lat_active();
            if self.flags & STEER_RATE_LIMITED != 0 {
                self.snapshot.steer_rate_counter =
                    if request && out.get_steering_rate_deg().abs() > 25. {
                        self.snapshot
                            .steer_rate_counter
                            .checked_add(1)
                            .ok_or(Error::Numeric)?
                    } else {
                        0
                    };
                if self.snapshot.steer_rate_counter > 7 {
                    request = false;
                }
                if self.snapshot.steer_rate_counter >= 8 {
                    self.snapshot.steer_rate_counter = 0;
                }
            }
            can::send(
                &mut self.packer,
                "ES_LKAS",
                0,
                &[
                    ("LKAS_Output", f64::from(torque)),
                    ("LKAS_Request", f64::from(request)),
                    ("SET_1", 1.),
                ],
            )?
        };
        self.snapshot.apply_torque_last = torque;
        Ok(sent)
    }
    pub fn apply(&mut self, state: &State, input: ApplyInput<'_>) -> Result<ApplyOutput, Error> {
        let mut sends = Vec::new();
        let cc = input.control;
        if self.snapshot.frame.is_multiple_of(2) {
            sends.push(self.steering(state, cc)?);
        }
        self.cruise(state, cc, &mut sends)?;
        let mut message = Message::new_default();
        message.set_root(cc.get_actuators()?)?;
        let mut result = message.get_root::<actuators::Builder>()?;
        result.set_torque(
            (f64::from(self.snapshot.apply_torque_last) / f64::from(self.maximum))
                .to_f32()
                .ok_or(Error::Numeric)?,
        );
        result.set_torque_output_can(
            self.snapshot
                .apply_torque_last
                .to_f32()
                .ok_or(Error::Numeric)?,
        );
        self.snapshot.frame = self.snapshot.frame.checked_add(1).ok_or(Error::Numeric)?;
        self.logs.extend(
            std::mem::take(&mut self.packer.diagnostics)
                .into_iter()
                .map(|diagnostic| VehicleLog {
                    level: crate::query::DiagnosticLevel::Error,
                    message: diagnostic.message,
                }),
        );
        Ok(ApplyOutput {
            actuators: message,
            can: sends,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openpilot_can::dbc::Dbc;
    use std::sync::Arc;

    #[test]
    fn nonfinite_torque_fails_before_driver_limiting() {
        let dbc = Dbc::parse("subaru_global_2017_generated", "").unwrap();
        let mut message = Message::new_default();
        let mut cp = message.init_root::<car_params::Builder>();
        cp.set_car_fingerprint("SUBARU_ASCENT");
        let controller = Controller::new(Packer::new(Arc::new(dbc)), cp.into_reader()).unwrap();
        for command in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let result = controller.limited(command, 0.);
            assert!(matches!(result, Err(Error::Numeric)));
        }
    }
}
