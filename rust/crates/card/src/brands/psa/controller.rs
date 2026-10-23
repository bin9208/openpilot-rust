use super::{
    state::{float, State},
    Error,
};
use crate::core::{ApplyInput, ApplyOutput, Message};
use openpilot_can::{packer::Packer, Frame};
use openpilot_cereal::car_capnp::{car_control::actuators, car_state};
use openpilot_control_policy::math::{clip, interp};
use serde::Serialize;

#[derive(Serialize)]
pub struct Controller {
    pub frame: u64,
    pub apply_angle_last: f64,
    pub status: u8,
    #[serde(skip)]
    pub packer: Packer,
}
impl Controller {
    pub(super) fn new(packer: Packer) -> Self {
        Self {
            frame: 0,
            apply_angle_last: 0.,
            status: 2,
            packer,
        }
    }
    pub fn apply(&mut self, state: &State, input: ApplyInput<'_>) -> Result<ApplyOutput, Error> {
        let cc = input.control;
        let actuators = cc.get_actuators()?;
        let mut can = Vec::new();
        if self.frame.is_multiple_of(5) {
            let cs = state.out.get_root_as_reader::<car_state::Reader>()?;
            let requested = f64::from(actuators.get_steering_angle_deg());
            let last = self.apply_angle_last;
            let up = last * requested >= 0. && requested.abs() > last.abs();
            let rate = interp(
                f64::from(cs.get_v_ego_raw()),
                &[0., 5., 25.],
                if up { &[2.5, 1.5, 0.2] } else { &[5., 2., 0.3] },
            )?;
            let angle = clip(
                if cc.get_lat_active() {
                    clip(requested, last - rate, last + rate)
                } else {
                    f64::from(cs.get_steering_angle_deg())
                },
                -390.,
                390.,
            );
            if !cc.get_lat_active() {
                self.status = 2;
            } else if !state.eps_active.ok_or(Error::Stock("eps_active"))?
                && !cs.get_steering_pressed()
            {
                self.status = if self.status == 4 { 2 } else { self.status + 1 };
            } else {
                self.status = 4;
            }
            let data = self.packer.pack(
                "LANE_KEEP_ASSIST",
                &[
                    ("DRIVE", 1.),
                    ("STATUS", f64::from(self.status)),
                    ("LXA_ACTIVATION", 1.),
                    ("TORQUE_FACTOR", if cc.get_lat_active() { 100. } else { 0. }),
                    ("SET_ANGLE", angle),
                ],
                None,
            )?;
            can.push(Frame {
                address: self.packer.dbc.message("LANE_KEEP_ASSIST")?.address,
                data,
                bus: 0,
            });
            self.apply_angle_last = angle;
        }
        let mut result = Message::new_default();
        result.set_root(actuators)?;
        result
            .get_root::<actuators::Builder>()?
            .set_steering_angle_deg(float(self.apply_angle_last)?);
        self.frame = self.frame.checked_add(1).ok_or(Error::Numeric)?;
        Ok(ApplyOutput {
            actuators: result,
            can,
        })
    }
}
