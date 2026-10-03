use super::{can, state::State, Candidate, Error};
use crate::core::{ApplyInput, ApplyOutput, Message};
use num_traits::ToPrimitive;
use openpilot_can::packer::Packer;
use openpilot_cereal::car_capnp::car_control::{actuators, h_u_d_control::VisualAlert};
use openpilot_control_policy::math::{clip, interp};
use serde::Serialize;

#[derive(Default, Serialize)]
pub struct Snapshot {
    pub frame: u64,
    pub apply_angle_last: f64,
}
pub struct Controller {
    pub snapshot: Snapshot,
    packer: Packer,
    candidate: Candidate,
}
impl Controller {
    pub(super) fn new(packer: Packer, candidate: Candidate) -> Self {
        Self {
            snapshot: Snapshot::default(),
            packer,
            candidate,
        }
    }
    pub fn apply(&mut self, state: &State, input: ApplyInput<'_>) -> Result<ApplyOutput, Error> {
        let cc = input.control;
        let actuators = cc.get_actuators()?;
        let hud = cc.get_hud_control()?;
        let cancel = cc.get_cruise_control()?.get_cancel();
        let out = state
            .out
            .get_root_as_reader::<openpilot_cereal::car_capnp::car_state::Reader>()?;
        let requested = f64::from(actuators.get_steering_angle_deg());
        let last = self.snapshot.apply_angle_last;
        let up = last * requested >= 0. && requested.abs() > last.abs();
        let rates = if up { [5., 0.8, 0.15] } else { [5., 3.5, 0.4] };
        let rate = interp(f64::from(out.get_v_ego_raw()), &[0., 5., 15.], &rates)?;
        let angle = clip(requested, last - rate, last + rate);
        self.snapshot.apply_angle_last = clip(
            if cc.get_lat_active() {
                angle
            } else {
                f64::from(out.get_steering_angle_deg())
            },
            -600.,
            600.,
        );
        let max_torque = if !cc.get_lat_active() {
            0.
        } else if !out.get_steering_pressed() {
            1.
        } else {
            f64::max(
                0.5,
                1. - 0.6 * f64::max(0., f64::from(out.get_steering_torque()).abs() - 1.),
            )
        };
        let mut can_sends = Vec::new();
        if !self.candidate.leaf() && cancel {
            can_sends.push(can::acc_cancel(
                &mut self.packer,
                self.candidate,
                &state.extras.cruise_throttle_msg,
            )?);
        }
        let frame = self.snapshot.frame;
        if self.candidate.leaf() && frame.is_multiple_of(2) {
            can_sends.push(can::leaf_cancel(
                &mut self.packer,
                &state.extras.cancel_msg,
                cancel,
            )?);
        }
        can_sends.push(can::steering(
            &mut self.packer,
            self.snapshot.apply_angle_last,
            frame,
            cc.get_lat_active(),
            max_torque,
        )?);
        if !self.candidate.altima() {
            if frame.is_multiple_of(2) {
                can_sends.push(can::hud(
                    &mut self.packer,
                    &state.extras.lkas_hud_msg,
                    hud,
                    cc.get_enabled(),
                )?);
            }
            if frame.is_multiple_of(50) {
                let required = matches!(
                    hud.get_visual_alert()?,
                    VisualAlert::SteerRequired | VisualAlert::Ldw
                );
                can_sends.push(can::hud_info(
                    &mut self.packer,
                    &state.extras.lkas_hud_info_msg,
                    required,
                )?);
            }
        }
        let mut message = Message::new_default();
        message.set_root(actuators)?;
        message
            .get_root::<actuators::Builder>()?
            .set_steering_angle_deg(
                self.snapshot
                    .apply_angle_last
                    .to_f32()
                    .ok_or(Error::Numeric)?,
            );
        self.snapshot.frame = frame.checked_add(1).ok_or(Error::Numeric)?;
        Ok(ApplyOutput {
            actuators: message,
            can: can_sends,
        })
    }
}
