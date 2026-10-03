use super::{can, state::State, Error};
use crate::core::{ApplyInput, ApplyOutput, Message, VehicleLog};
use num_traits::ToPrimitive;
use openpilot_can::packer::Packer;
use openpilot_cereal::car_capnp::{car_control::actuators, car_state};
use openpilot_control_policy::math::{clip, interp};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Default, Serialize)]
pub struct Snapshot {
    pub frame: u64,
    pub apply_torque_last: i32,
    pub cancel_frames: u64,
}
pub struct Controller {
    pub snapshot: Snapshot,
    pub logs: Vec<VehicleLog>,
    packer: Packer,
    longitudinal: bool,
}
fn driver_limit(command: f64, previous: i32, driver: f32, maximum: i32) -> Result<i32, Error> {
    if !command.is_finite() {
        return Err(Error::Numeric);
    }
    let maximum = f64::from(maximum);
    let max_driver = maximum + (100. + f64::from(driver) * 100.) * 2.;
    let min_driver = -maximum + (-100. + f64::from(driver) * 100.) * 2.;
    let command = clip(
        command,
        (-maximum).max(min_driver).min(0.),
        maximum.min(max_driver).max(0.),
    );
    let previous = f64::from(previous);
    let command = if previous > 0. {
        clip(command, (previous - 5.).max(-3.), previous + 3.)
    } else {
        clip(command, previous - 3., (previous + 5.).min(3.))
    };
    command.round_ties_even().to_i32().ok_or(Error::Numeric)
}
impl Controller {
    pub fn new(packer: Packer, longitudinal: bool) -> Self {
        Self {
            snapshot: Snapshot::default(),
            logs: Vec::new(),
            packer,
            longitudinal,
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
        let out = state.out.get_root_as_reader::<car_state::Reader>()?;
        let maximum = interp(f64::from(out.get_v_ego_raw()), &[9., 17.], &[350., 250.])?
            .round_ties_even()
            .to_i32()
            .ok_or(Error::Numeric)?;
        let torque = if cc.get_lat_active() {
            driver_limit(
                (f64::from(actuators.get_torque()) * f64::from(maximum)).round_ties_even(),
                self.snapshot.apply_torque_last,
                out.get_steering_torque(),
                maximum,
            )?
        } else {
            0
        };
        self.snapshot.apply_torque_last = torque;
        let frame = self.snapshot.frame;
        let stock = state
            .extras
            .acm_lka_hba_cmd
            .as_ref()
            .ok_or(Error::Stock("ACM_lkaHbaCmd"))?;
        let mut sends = vec![can::steering(
            &mut self.packer,
            frame,
            stock,
            torque,
            cc.get_enabled(),
            cc.get_lat_active(),
        )?];
        if frame.is_multiple_of(5) {
            sends.push(can::wheel_touch(
                &mut self.packer,
                state
                    .extras
                    .sccm_wheel_touch
                    .as_ref()
                    .ok_or(Error::Stock("SCCM_WheelTouch"))?,
                cc.get_enabled(),
            )?);
        }
        if self.longitudinal {
            sends.push(can::longitudinal(
                &mut self.packer,
                frame,
                clip(f64::from(actuators.get_accel()), -3.5, 2.),
                cc.get_enabled(),
            )?);
        } else {
            let status = if cc.get_cruise_control()?.get_cancel() {
                let status = if self.snapshot.cancel_frames < 5 {
                    1.
                } else {
                    0.
                };
                self.snapshot.cancel_frames = self
                    .snapshot
                    .cancel_frames
                    .checked_add(1)
                    .ok_or(Error::Numeric)?;
                Some(status)
            } else {
                self.snapshot.cancel_frames = 0;
                None
            };
            sends.push(can::adas(
                &mut self.packer,
                state
                    .extras
                    .vdm_adas_status
                    .as_ref()
                    .ok_or(Error::Stock("VDM_AdasSts"))?,
                status,
            )?);
        }
        let mut message = Message::new_default();
        message.set_root(actuators)?;
        let mut a = message.get_root::<actuators::Builder>()?;
        a.set_torque(
            (f64::from(torque) / f64::from(maximum))
                .to_f32()
                .ok_or(Error::Numeric)?,
        );
        a.set_torque_output_can(torque.to_f32().ok_or(Error::Numeric)?);
        self.snapshot.frame = frame.checked_add(1).ok_or(Error::Numeric)?;
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
            can: sends,
        })
    }
}
