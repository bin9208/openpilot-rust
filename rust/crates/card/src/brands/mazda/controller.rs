use super::{can, state::State, Error};
use crate::{
    brands::hyundai::parameters::setting_int,
    core::{ApplyInput, ApplyOutput},
};
use num_bigint::BigInt;
use num_traits::{FromPrimitive, ToPrimitive};
use openpilot_can::packer::Packer;
use openpilot_cereal::car_capnp::car_control::{self, h_u_d_control::VisualAlert};
use openpilot_control_policy::math::clip;
use openpilot_params::Params;
use serde::Serialize;

#[derive(Default, Serialize)]
pub struct Snapshot {
    pub frame: u64,
    pub apply_torque_last: i32,
    pub brake_counter: u64,
    #[serde(rename = "activateCruise")]
    pub activate_cruise: i32,
    pub speed_from_pcm: i32,
}
pub struct Controller {
    pub snapshot: Snapshot,
    pub prints: Vec<String>,
    packer: Packer,
    flags: u32,
    settings: Params,
}

fn driver_limit(command: f64, previous: i32, driver: f32) -> Result<i32, Error> {
    if !command.is_finite() {
        return Err(Error::Numeric);
    }
    let maximum = 800. + (15. + f64::from(driver));
    let minimum = -800. + (-15. + f64::from(driver));
    let command = clip(
        command,
        (-800_f64).max(minimum).min(0.),
        800_f64.min(maximum).max(0.),
    );
    let previous = f64::from(previous);
    let command = if previous > 0. {
        clip(command, (previous - 25.).max(-10.), previous + 10.)
    } else {
        clip(command, previous - 10., (previous + 25.).min(10.))
    };
    command.round_ties_even().to_i32().ok_or(Error::Numeric)
}
fn speed_target(value: f64) -> Result<BigInt, Error> {
    let integer = BigInt::from_f64((value + 0.5).trunc()).ok_or(Error::Numeric)?;
    let rounded =
        BigInt::from_f64((integer.to_f64().ok_or(Error::Numeric)? / 5.).round_ties_even())
            .ok_or(Error::Numeric)?;
    BigInt::from_f64(rounded.to_f64().ok_or(Error::Numeric)? * 5.).ok_or(Error::Numeric)
}
impl Controller {
    pub fn new(packer: Packer, flags: u32, settings: Params) -> Self {
        Self {
            snapshot: Snapshot {
                speed_from_pcm: 1,
                ..Snapshot::default()
            },
            prints: Vec::new(),
            packer,
            flags,
            settings,
        }
    }
    fn spam_button(
        &mut self,
        control: car_control::Reader<'_>,
        state: &State,
    ) -> Result<i32, Error> {
        let cs = state
            .out
            .get_root_as_reader::<openpilot_cereal::car_capnp::car_state::Reader>()?;
        let hud = control.get_hud_control()?;
        let units = if state.is_metric {
            3.6
        } else {
            1. / (1.609344 * (1. / 3.6))
        };
        let target = speed_target(f64::from(hud.get_set_speed()) * units)?;
        let current = speed_target(f64::from(cs.get_cruise_state()?.get_speed()) * 3.6)?;
        let can_activate = (hud.get_lead_visible() || f64::from(cs.get_v_ego()) * 3.6 > 10.)
            && self.snapshot.activate_cruise == 0
            && !cs.get_brake_pressed()
            && !cs.get_gas_pressed();
        if control.get_enabled() {
            if !cs.get_cruise_state()?.get_enabled() {
                if can_activate {
                    self.snapshot.activate_cruise = 1;
                    self.prints.push("RESUME".into());
                    return Ok(3);
                }
            } else if control.get_cruise_control()?.get_resume() {
                return Ok(3);
            } else if target < current
                && current >= BigInt::from(31)
                && self.snapshot.speed_from_pcm != 1
            {
                self.prints
                    .push(format!("SET_MINUS target={target}, current={current}"));
                return Ok(2);
            } else if target > current
                && current < BigInt::from(160)
                && self.snapshot.speed_from_pcm != 1
            {
                self.prints
                    .push(format!("SET_PLUS target={target}, current={current}"));
                return Ok(1);
            }
        } else if cs.get_activate_cruise() != 0 && can_activate {
            self.snapshot.activate_cruise = 1;
            self.prints.push("RESUME".into());
            return Ok(3);
        }
        Ok(0)
    }
    pub fn apply(&mut self, state: &State, input: ApplyInput<'_>) -> Result<ApplyOutput, Error> {
        let control = input.control;
        if self.snapshot.frame.is_multiple_of(50) {
            self.snapshot.speed_from_pcm =
                setting_int(&self.settings, "SpeedFromPCM").map_err(|error| match error {
                    crate::brands::hyundai::Error::Params(error) => Error::Params(error),
                    _ => Error::Numeric,
                })?;
        }
        let cs = state
            .out
            .get_root_as_reader::<openpilot_cereal::car_capnp::car_state::Reader>()?;
        let actuators = control.get_actuators()?;
        let torque = if control.get_lat_active() {
            let requested = (f64::from(actuators.get_torque()) * 800.).round_ties_even();
            driver_limit(
                requested,
                self.snapshot.apply_torque_last,
                cs.get_steering_torque(),
            )?
        } else {
            0
        };
        let mut sends = Vec::new();
        if control.get_cruise_control()?.get_cancel() {
            self.snapshot.brake_counter = self
                .snapshot
                .brake_counter
                .checked_add(1)
                .ok_or(Error::Numeric)?;
            if self.snapshot.frame.is_multiple_of(10)
                && !(cs.get_brake_pressed() && self.snapshot.brake_counter < 7)
            {
                sends.push(can::button(
                    &mut self.packer,
                    self.flags,
                    state
                        .extras
                        .crz_btns_counter
                        .to_u64()
                        .ok_or(Error::Numeric)?,
                    4,
                )?);
            }
        } else if self.snapshot.frame.is_multiple_of(20) {
            let button = self.spam_button(control, state)?;
            if button > 0 {
                self.snapshot.brake_counter = 0;
                sends.push(can::button(
                    &mut self.packer,
                    self.flags,
                    self.snapshot.frame / 10,
                    button,
                )?);
            }
        }
        self.snapshot.apply_torque_last = torque;
        if self.snapshot.frame.is_multiple_of(50) {
            let required = control.get_hud_control()?.get_visual_alert()?
                == VisualAlert::SteerRequired
                && state.extras.lkas_allowed_speed;
            sends.push(can::alert(
                &mut self.packer,
                &state.extras.cam_laneinfo,
                required,
            )?);
        }
        sends.push(can::steering(
            &mut self.packer,
            self.flags,
            self.snapshot.frame,
            torque,
            &state.extras.cam_lkas,
        )?);
        let mut output = crate::core::Message::new_default();
        output.set_root(actuators)?;
        let mut projected = output.get_root::<car_control::actuators::Builder>()?;
        projected.set_torque((f64::from(torque) / 800.) as f32);
        projected.set_torque_output_can(torque as f32);
        self.snapshot.frame = self.snapshot.frame.checked_add(1).ok_or(Error::Numeric)?;
        Ok(ApplyOutput {
            actuators: output,
            can: sends,
        })
    }
}
