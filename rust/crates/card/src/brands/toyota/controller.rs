use super::controller_history::{History, Snapshot};
use super::{config::Config, secoc::Key, state::State, Error, RAISED_ACCEL_LIMIT, TSS2};
use crate::core::{ApplyInput, ApplyOutput, Message, VehicleLog};
use num_traits::ToPrimitive;
use openpilot_can::packer::Packer;
use openpilot_cereal::car_capnp::{car_control::actuators, car_params};
use openpilot_control_policy::pid::{Gain, Gains, Pid};
use openpilot_params::Params;
use std::collections::BTreeMap;

pub struct Controller {
    pub history: History,
    pub logs: Vec<VehicleLog>,
    pub(super) packer: Packer,
    pub(super) config: Config,
    pub(super) key: Key,
    pub(super) maximum: i32,
    pub(super) delta_up: i32,
    pub(super) delta_down: i32,
    pub(super) accel_max: f64,
    pub(super) pid: Pid,
    pub(super) pid_speed: f64,
    pub(super) aego: f64,
    pub(super) pitch: f64,
    settings: Params,
}
impl Controller {
    pub fn new(
        packer: Packer,
        cp: car_params::Reader<'_>,
        settings: Params,
    ) -> Result<Self, Error> {
        let config = Config::new(cp)?;
        let accel_max = if config.flags & RAISED_ACCEL_LIMIT != 0 {
            2.
        } else {
            1.5
        };
        let i = if config.static_flags & TSS2 != 0 {
            Gain {
                x: vec![2., 5.],
                y: vec![0.5, 0.25],
            }
        } else {
            Gain {
                x: vec![0., 5., 35.],
                y: vec![3.6, 2.4, 1.5],
            }
        };
        let mut pid = Pid::new(
            Gains {
                p: Gain::constant(0.),
                i,
                d: Gain::constant(0.),
                f: 1.,
            },
            [-3.5, accel_max],
        );
        pid.rate = 1. / (0.01 * 3.);
        let torque = matches!(
            cp.get_lateral_tuning().which()?,
            car_params::lateral_tuning::Which::Torque(_)
        );
        Ok(Self {
            history: History::default(),
            logs: Vec::new(),
            packer,
            config,
            key: Key::default(),
            maximum: 1500,
            delta_up: if torque { 15 } else { 10 },
            delta_down: 25,
            accel_max,
            pid,
            pid_speed: 0.,
            aego: 0.,
            pitch: 0.,
            settings,
        })
    }
    pub fn snapshot(&self) -> Snapshot<'_> {
        Snapshot {
            history: &self.history,
            aego: self.aego,
            pitch: self.pitch,
            steer_max: self.maximum,
            steer_delta_up: self.delta_up,
            steer_delta_down: self.delta_down,
            pid: [
                self.pid.p,
                self.pid.i,
                self.pid.d,
                self.pid.f,
                self.pid.control,
                self.pid_speed,
            ],
        }
    }
    pub fn packer_counters(&self) -> BTreeMap<u32, String> {
        self.packer
            .counters
            .iter()
            .map(|(address, value)| (*address, value.to_string()))
            .collect()
    }
    pub fn apply(&mut self, state: &State, input: ApplyInput<'_>) -> Result<ApplyOutput, Error> {
        let cc = input.control;
        let orientation = cc.get_orientation_n_e_d()?;
        if orientation.len() == 3 {
            let alpha = 0.01 / (0.5 + 0.01);
            self.pitch = (1. - alpha) * self.pitch + alpha * f64::from(orientation.get(1));
        }
        let read = |key| {
            crate::brands::hyundai::parameters::setting_int(&self.settings, key)
                .map_err(|source| Error::SettingInteger { key, source })
        };
        let maximum = read("CustomSteerMax")?;
        let up = read("CustomSteerDeltaUp")?;
        let down = read("CustomSteerDeltaDown")?;
        if maximum > 0 {
            self.maximum = maximum;
        }
        if up > 0 {
            self.delta_up = up;
        }
        if down > 0 {
            self.delta_down = down;
        }
        let mut sends = self.steering(state, cc)?;
        self.longitudinal(state, cc, &mut sends)?;
        self.alerts(state, cc, &mut sends)?;
        let mut message = Message::new_default();
        message.set_root(cc.get_actuators()?)?;
        let mut result = message.get_root::<actuators::Builder>()?;
        result.set_torque(
            (f64::from(self.history.last_torque) / f64::from(self.maximum))
                .to_f32()
                .ok_or(Error::Numeric)?,
        );
        result.set_torque_output_can(self.history.last_torque.to_f32().ok_or(Error::Numeric)?);
        result.set_steering_angle_deg(self.history.last_angle.to_f32().ok_or(Error::Numeric)?);
        result.set_accel(self.history.accel.to_f32().ok_or(Error::Numeric)?);
        self.history.frame = self.history.frame.checked_add(1).ok_or(Error::Numeric)?;
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
