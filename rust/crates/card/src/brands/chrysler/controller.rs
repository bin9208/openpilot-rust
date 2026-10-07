use super::{can, state::State, Candidate, Error};
use crate::core::{ApplyInput, ApplyOutput, Message, VehicleLog};
use num_traits::ToPrimitive;
use openpilot_can::packer::Packer;
use openpilot_cereal::car_capnp::{car_control::actuators, car_state};
use openpilot_control_policy::math::clip;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Default, Serialize)]
pub struct Snapshot {
    pub frame: u64,
    pub apply_torque_last: i32,
    pub hud_count: u64,
    pub last_lkas_falling_edge: u64,
    pub lkas_control_bit_prev: bool,
    pub last_button_frame: u64,
}
pub struct Controller {
    pub snapshot: Snapshot,
    pub logs: Vec<VehicleLog>,
    packer: Packer,
    candidate: Candidate,
    flags: u32,
    min_speed: f64,
}
fn measured_limit(
    requested: f64,
    last: i32,
    motor: f32,
    maximum: i32,
    delta: i32,
) -> Result<i32, Error> {
    if !requested.is_finite() {
        return Err(Error::Numeric);
    }
    let maximum = f64::from(maximum);
    let upper = (f64::from(motor) + 80.).max(80.).min(maximum);
    let lower = (f64::from(motor) - 80.).min(-80.).max(-maximum);
    let command = clip(requested, lower, upper);
    let previous = f64::from(last);
    let delta = f64::from(delta);
    let command = if previous > 0. {
        clip(command, (previous - delta).max(-delta), previous + delta)
    } else {
        clip(command, previous - delta, (previous + delta).min(delta))
    };
    command.round_ties_even().to_i32().ok_or(Error::Numeric)
}
impl Controller {
    pub(super) fn new(packer: Packer, candidate: Candidate, flags: u32, min_speed: f32) -> Self {
        Self {
            snapshot: Snapshot::default(),
            logs: Vec::new(),
            packer,
            candidate,
            flags,
            min_speed: f64::from(min_speed),
        }
    }
    pub fn packer_counters(&self) -> BTreeMap<u32, String> {
        self.packer
            .counters
            .iter()
            .map(|(address, counter)| (*address, counter.to_string()))
            .collect()
    }
    pub fn apply(&mut self, state: &State, input: ApplyInput<'_>) -> Result<ApplyOutput, Error> {
        let cc = input.control;
        let actuators = cc.get_actuators()?;
        let cruise = cc.get_cruise_control()?;
        let out = state.out.get_root_as_reader::<car_state::Reader>()?;
        let frame = self.snapshot.frame;
        let active = cc.get_lat_active() && self.snapshot.lkas_control_bit_prev;
        let mut can_sends = Vec::new();
        let elapsed = frame
            .checked_sub(self.snapshot.last_button_frame)
            .ok_or(Error::Numeric)?
            .to_f64()
            .ok_or(Error::Numeric)?
            * 0.01;
        if elapsed > 0.05 && (cruise.get_cancel() || cruise.get_resume()) {
            self.snapshot.last_button_frame = frame;
            can_sends.push(can::buttons(
                &mut self.packer,
                state.extras.button_counter + 1.,
                if self.candidate.ram() { 2 } else { 0 },
                cruise.get_cancel(),
            )?);
        }
        if frame.is_multiple_of(25) && state.extras.lkas_car_model != -1. {
            can_sends.push(can::hud(
                &mut self.packer,
                can::Hud {
                    active,
                    alert: cc.get_hud_control()?.get_visual_alert()?,
                    count: self.snapshot.hud_count,
                    model: state.extras.lkas_car_model,
                    high_beam: state.extras.auto_high_beam,
                    ram: self.candidate.ram(),
                },
            )?);
            self.snapshot.hud_count = self
                .snapshot
                .hud_count
                .checked_add(1)
                .ok_or(Error::Numeric)?;
        }
        if frame.is_multiple_of(2) {
            let mut control = self.snapshot.lkas_control_bit_prev;
            let speed = f64::from(out.get_v_ego());
            if speed > self.min_speed {
                control = true;
            } else if self.flags & 1 != 0 {
                if speed < self.min_speed - 3. {
                    control = false;
                }
            } else if self.candidate.ram() && speed < self.min_speed - 0.5 {
                control = false;
            }
            control &= frame
                .checked_sub(self.snapshot.last_lkas_falling_edge)
                .ok_or(Error::Numeric)?
                > 200;
            if !control && self.snapshot.lkas_control_bit_prev {
                self.snapshot.last_lkas_falling_edge = frame;
            }
            self.snapshot.lkas_control_bit_prev = control;
            let maximum = self.candidate.torque_limit();
            let requested =
                (f64::from(actuators.get_torque()) * f64::from(maximum)).round_ties_even();
            let limited = measured_limit(
                requested,
                self.snapshot.apply_torque_last,
                out.get_steering_torque_eps(),
                maximum,
                self.candidate.torque_delta(),
            )?;
            self.snapshot.apply_torque_last = if active && control { limited } else { 0 };
            can_sends.push(can::steering(
                &mut self.packer,
                self.snapshot.apply_torque_last,
                control,
                self.candidate.ram(),
            )?);
        }
        self.snapshot.frame = frame.checked_add(1).ok_or(Error::Numeric)?;
        let mut message = Message::new_default();
        message.set_root(actuators)?;
        let mut a = message.get_root::<actuators::Builder>()?;
        a.set_torque(
            (f64::from(self.snapshot.apply_torque_last) / f64::from(self.candidate.torque_limit()))
                .to_f32()
                .ok_or(Error::Numeric)?,
        );
        a.set_torque_output_can(
            self.snapshot
                .apply_torque_last
                .to_f32()
                .ok_or(Error::Numeric)?,
        );
        self.logs.extend(
            std::mem::take(&mut self.packer.diagnostics)
                .into_iter()
                .map(|d| VehicleLog {
                    level: crate::query::DiagnosticLevel::Error,
                    message: d.message,
                }),
        );
        Ok(ApplyOutput {
            actuators: message,
            can: can_sends,
        })
    }
}
