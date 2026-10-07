use super::{
    parameters::{parameters, ParamsInput},
    speed_limit::SpeedLimit,
    state::State,
    steering::{self, Coop, SteeringInput},
    Error, AUTO_SPEED_LIMIT, FSD_14,
};
use crate::core::{ApplyInput, ApplyOutput, Message};
use num_traits::ToPrimitive;
use openpilot_can::{packer::Packer, Frame};
use openpilot_cereal::car_capnp::{car_control, car_params, car_state};
use openpilot_control_policy::{
    math::clip,
    vehicle::{Physical, VehicleModel},
};
use openpilot_params::Params;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct Snapshot<'a> {
    pub frame: u64,
    pub apply_angle_last: f64,
    pub l_jerk: f64,
    pub coop: &'a Coop,
    pub speed_limit: &'a SpeedLimit,
}
pub struct Controller {
    pub frame: u64,
    pub apply_angle_last: f64,
    pub l_jerk: f64,
    pub coop: Coop,
    pub speed_limit: SpeedLimit,
    packer: Packer,
    vm: VehicleModel,
    flags: u32,
    longitudinal: bool,
}
impl Controller {
    pub fn new(
        cp: car_params::Reader<'_>,
        state: &State,
        settings: &Params,
    ) -> Result<Self, Error> {
        let safety = parameters(ParamsInput {
            candidate: "TESLA_MODEL_Y",
            fingerprints: &[],
            firmware: &[],
            alpha_long: false,
            settings,
        })?;
        let p = safety.get_root_as_reader::<car_params::Reader>()?;
        let vm = VehicleModel::new(Physical {
            mass: f64::from(p.get_mass()),
            inertia: f64::from(p.get_rotational_inertia()),
            wheelbase: f64::from(p.get_wheelbase()),
            center_front: f64::from(p.get_center_to_front()),
            rear_ratio: f64::from(p.get_steer_ratio_rear()),
            stiffness_front: f64::from(p.get_tire_stiffness_front()),
            stiffness_rear: f64::from(p.get_tire_stiffness_rear()),
            steer_ratio: f64::from(p.get_steer_ratio()),
        });
        Ok(Self {
            frame: 0,
            apply_angle_last: 0.,
            l_jerk: 0.,
            coop: Coop::default(),
            speed_limit: SpeedLimit {
                configured: cp.get_flags() & AUTO_SPEED_LIMIT != 0,
                ..SpeedLimit::default()
            },
            packer: Packer::new(Arc::clone(&state.party.dbc)),
            vm,
            flags: cp.get_flags(),
            longitudinal: cp.get_openpilot_longitudinal_control(),
        })
    }
    pub fn snapshot(&self) -> Snapshot<'_> {
        Snapshot {
            frame: self.frame,
            apply_angle_last: self.apply_angle_last,
            l_jerk: self.l_jerk,
            coop: &self.coop,
            speed_limit: &self.speed_limit,
        }
    }
    fn frame(
        &mut self,
        name: &str,
        values: &[(&str, f64)],
        checksum: &str,
        count: usize,
    ) -> Result<Frame, Error> {
        let address = self.packer.dbc.message(name)?.address;
        let first = self.packer.pack(name, values, None)?;
        let sum = (address & 255)
            + ((address >> 8) & 255)
            + first.iter().take(count).map(|b| u32::from(*b)).sum::<u32>();
        let mut values = values.to_vec();
        values.push((checksum, f64::from(sum & 255)));
        Ok(Frame {
            address,
            data: self.packer.pack(name, &values, None)?,
            bus: 0,
        })
    }
    fn longitudinal(
        &mut self,
        state: f64,
        accel: f64,
        counter: f64,
        speed: f64,
        active: bool,
        override_cruise: bool,
    ) -> Result<Frame, Error> {
        let mut set = (speed * 3.6).max(0.);
        if active {
            self.l_jerk = if override_cruise {
                0.
            } else {
                self.l_jerk + 1. * 0.01 * 4.
            };
            set = ((speed + accel).max(0.) * 3.6).min(145.);
        } else {
            self.l_jerk = 0.;
        }
        self.frame(
            "DAS_control",
            &[
                ("DAS_setSpeed", set),
                ("DAS_accState", state),
                ("DAS_aebEvent", 0.),
                ("DAS_jerkMin", -4.9),
                ("DAS_jerkMax", self.l_jerk.min(4.9)),
                ("DAS_accelMin", accel),
                ("DAS_accelMax", accel.max(0.)),
                ("DAS_controlCounter", counter),
                ("DAS_controlChecksum", 0.),
            ],
            "DAS_controlChecksum",
            7,
        )
    }
    pub fn apply(
        &mut self,
        state: &mut State,
        input: ApplyInput<'_>,
    ) -> Result<ApplyOutput, Error> {
        let cc = input.control;
        let actuators = cc.get_actuators()?;
        let mut can = self.speed_limit.update(cc, state, input.now_ns)?;
        let cs = state.out.get_root_as_reader::<car_state::Reader>()?;
        let lat = cc.get_lat_active() && !state.extras.steering_disengage;
        if self.frame.is_multiple_of(2) {
            self.apply_angle_last = steering::limit(
                f64::from(actuators.get_steering_angle_deg()),
                self.apply_angle_last,
                f64::from(cs.get_v_ego_raw()),
                f64::from(cs.get_steering_angle_deg()),
                lat,
                &self.vm,
            )?;
            let angle = self.coop.update(
                SteeringInput {
                    requested: self.apply_angle_last,
                    active: lat,
                    speed: f64::from(cs.get_v_ego()),
                    raw_speed: f64::from(cs.get_v_ego_raw()),
                    steering_angle: f64::from(cs.get_steering_angle_deg()),
                    steering_rate: f64::from(cs.get_steering_rate_deg()),
                    torque: f64::from(cs.get_steering_torque()),
                },
                &self.vm,
            )?;
            let mut kind = if lat { 1. } else { 0. };
            if self.flags & FSD_14 != 0 && kind == 1. {
                kind = 2.;
            }
            can.push(self.frame(
                "DAS_steeringControl",
                &[
                    ("DAS_steeringAngleRequest", -angle),
                    ("DAS_steeringHapticRequest", 0.),
                    ("DAS_steeringControlType", kind),
                    (
                        "DAS_steeringControlCounter",
                        f64::from(u8::try_from((self.frame / 2) % 16).map_err(|_| Error::Numeric)?),
                    ),
                ],
                "DAS_steeringControlChecksum",
                3,
            )?);
        }
        if self.frame.is_multiple_of(10) {
            can.push(self.frame(
                "APS_eacMonitor",
                &[
                    ("APS_eacAllow", 1.),
                    (
                        "APS_eacMonitorCounter",
                        f64::from(
                            u8::try_from((self.frame / 10) % 16).map_err(|_| Error::Numeric)?,
                        ),
                    ),
                ],
                "APS_eacMonitorChecksum",
                2,
            )?);
        }
        if self.longitudinal {
            if self.frame.is_multiple_of(4) {
                let cancel = cc.get_cruise_control()?.get_cancel() || state.extras.das_acc_cancel;
                let accel = if cc.get_long_active() {
                    clip(f64::from(actuators.get_accel()), -3.48, 2.)
                } else {
                    0.
                };
                let counter =
                    f64::from(u8::try_from((self.frame / 4) % 8).map_err(|_| Error::Numeric)?);
                can.push(self.longitudinal(
                    if cancel { 13. } else { 4. },
                    accel,
                    counter,
                    f64::from(cs.get_v_ego()),
                    cc.get_long_active(),
                    state.extras.cruise_override,
                )?);
            }
        } else if cc.get_cruise_control()?.get_cancel() {
            let counter = (state
                .autopilot
                .signal("DAS_control", "DAS_controlCounter")?
                + 1.)
                .rem_euclid(8.);
            can.push(self.longitudinal(
                13.,
                0.,
                counter,
                f64::from(cs.get_v_ego()),
                false,
                true,
            )?);
        }
        let mut out = Message::new_default();
        out.set_root(actuators)?;
        out.get_root::<car_control::actuators::Builder>()?
            .set_steering_angle_deg(self.apply_angle_last.to_f32().ok_or(Error::Numeric)?);
        self.frame += 1;
        Ok(ApplyOutput {
            actuators: out,
            can,
        })
    }
}
