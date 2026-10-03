use super::{
    can, can_ui,
    config::Config,
    controller_history::{History, Limits, Snapshot},
    controller_long::Longitudinal,
    state::{float, State},
    Error,
};
use crate::core::{ApplyInput, ApplyOutput, Message, VehicleLog};
use num_traits::ToPrimitive;
use openpilot_can::{packer::Packer, Frame};
use openpilot_cereal::car_capnp::{car_control::actuators, car_params};
use openpilot_params::Params;
use std::collections::BTreeMap;

pub struct Controller {
    pub history: History,
    pub limits: Limits,
    pub logs: Vec<VehicleLog>,
    pub(super) packer: Packer,
    pub(super) config: Config,
    settings: Params,
}
impl Controller {
    pub fn new(
        packer: Packer,
        cp: car_params::Reader<'_>,
        settings: Params,
    ) -> Result<Self, Error> {
        Ok(Self {
            history: History::default(),
            limits: Limits::new(cp)?,
            logs: Vec::new(),
            packer,
            config: Config::new(cp)?,
            settings,
        })
    }
    pub fn snapshot(&self) -> Snapshot<'_> {
        Snapshot {
            history: &self.history,
            limits: &self.limits,
        }
    }
    pub fn packer_counters(&self) -> BTreeMap<u32, String> {
        self.packer
            .counters
            .iter()
            .map(|(k, v)| (*k, v.to_string()))
            .collect()
    }
    fn settings(&mut self) -> Result<(), Error> {
        if self.history.frame.is_multiple_of(50) {
            let read = |key| {
                crate::brands::hyundai::parameters::setting_int(&self.settings, key)
                    .map_err(|source| Error::SettingInteger { key, source })
            };
            let max = read("CustomSteerMax")?;
            let up = read("CustomSteerDeltaUp")?;
            let down = read("CustomSteerDeltaDown")?;
            if max > 0 {
                self.limits.steer_max = f64::from(max);
                self.limits.steer_lookup_bp = vec![0., f64::from(max)];
                self.limits.steer_lookup_v = vec![0., f64::from(max)];
            }
            if up > 0 {
                self.limits.steer_delta_up = up;
            }
            if down > 0 {
                self.limits.steer_delta_down = down;
            }
        }
        Ok(())
    }
    pub fn apply(&mut self, state: &State, input: ApplyInput<'_>) -> Result<ApplyOutput, Error> {
        self.settings()?;
        let cc = input.control;
        let act = cc.get_actuators()?;
        let hud = cc.get_hud_control()?;
        let cruise = if hud.get_speed_visible() {
            f64::from(hud.get_set_speed()) / self.config.conversion(state.is_metric)
        } else {
            255.
        };
        let prepared = self.prepare(state, cc)?;
        let torque = prepared.torque;
        let mut sends = Vec::new();
        if self.config.bosch()
            && !self.config.radarless()
            && self.config.longitudinal
            && self.history.frame.is_multiple_of(10)
        {
            sends.push(Frame {
                address: 0x18dab0f1,
                bus: 1,
                data: vec![2, 0x3e, 0x80, 0, 0, 0, 0, 0],
            });
        }
        sends.push(can::send(
            &mut self.packer,
            "STEERING_CONTROL",
            self.config.bus.lkas,
            &[
                (
                    "STEER_TORQUE",
                    if cc.get_lat_active() {
                        torque.to_f64().ok_or(Error::Numeric)?
                    } else {
                        0.
                    },
                ),
                ("STEER_TORQUE_REQUEST", f64::from(cc.get_lat_active())),
            ],
        )?);
        let [pcm_speed, pcm_accel, wind] = self.pcm(
            state,
            prepared.accel,
            prepared.gas_brake,
            cc.get_long_active(),
        )?;
        self.longitudinal(
            state,
            Longitudinal {
                control: cc,
                accel: prepared.accel,
                wind,
                fcw: prepared.fcw,
            },
            &mut sends,
        )?;
        if self.history.frame.is_multiple_of(10) {
            let cruise = cruise.round_ties_even();
            if !cruise.is_finite() {
                return Err(Error::Numeric);
            }
            sends.extend(can_ui::ui(
                &mut self.packer,
                &self.config,
                can_ui::Ui {
                    enabled: cc.get_enabled(),
                    pcm_speed,
                    pcm_accel: pcm_accel.to_i32().ok_or(Error::Numeric)?,
                    cruise,
                    hud,
                    steer: prepared.steer,
                    metric: state.is_metric,
                    acc_hud: state
                        .extras
                        .acc_hud
                        .as_ref()
                        .ok_or(Error::Stock("acc_hud"))?,
                    lkas_hud: state
                        .extras
                        .lkas_hud
                        .as_ref()
                        .ok_or(Error::Stock("lkas_hud"))?,
                },
            )?);
            if self.config.longitudinal && !self.config.bosch() {
                self.history.speed = pcm_speed;
                self.history.gas = pcm_accel / 198.;
            }
        }
        let mut message = Message::new_default();
        message.set_root(act)?;
        let mut output = message.get_root::<actuators::Builder>()?;
        output.set_speed(float(self.history.speed)?);
        output.set_accel(float(self.history.accel)?);
        output.set_gas(float(self.history.gas)?);
        output.set_brake(float(self.history.brake)?);
        output.set_torque(float(self.history.last_torque)?);
        output.set_torque_output_can(float(torque.to_f64().ok_or(Error::Numeric)?)?);
        self.history.frame = self.history.frame.checked_add(1).ok_or(Error::Numeric)?;
        self.drain_logs();
        Ok(ApplyOutput {
            actuators: message,
            can: sends,
        })
    }
    pub(super) fn drain_logs(&mut self) {
        self.logs.extend(
            std::mem::take(&mut self.packer.diagnostics)
                .into_iter()
                .map(|d| VehicleLog {
                    level: crate::query::DiagnosticLevel::Error,
                    message: d.message,
                }),
        );
    }
}
