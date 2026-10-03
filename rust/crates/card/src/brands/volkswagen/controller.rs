use super::{
    can_aux, can_buttons,
    config::{Config, Family},
    controller_history::History,
    state::{float, State},
    Error, STOCK_EA_PRESENT, STOCK_KLR_PRESENT,
};
use crate::core::{ApplyInput, ApplyOutput, Message, VehicleLog};
use openpilot_can::packer::Packer;
use openpilot_cereal::car_capnp::{car_control::actuators, car_params};
use std::collections::BTreeMap;
pub struct Controller {
    pub history: History,
    pub logs: Vec<VehicleLog>,
    pub(super) packer: Packer,
    pub(super) config: Config,
}
impl Controller {
    pub fn new(packer: Packer, cp: car_params::Reader<'_>) -> Result<Self, Error> {
        Ok(Self {
            history: History::default(),
            logs: Vec::new(),
            packer,
            config: Config::new(cp)?,
        })
    }
    pub fn snapshot(&self) -> &History {
        &self.history
    }
    pub fn packer_counters(&self) -> BTreeMap<u32, String> {
        self.packer
            .counters
            .iter()
            .map(|(k, v)| (*k, v.to_string()))
            .collect()
    }
    pub fn apply(
        &mut self,
        state: &mut State,
        input: ApplyInput<'_>,
    ) -> Result<ApplyOutput, Error> {
        let cc = input.control;
        let act = cc.get_actuators()?;
        let mut sends = self.steering(state, cc)?;
        if self.config.flags & STOCK_KLR_PRESENT != 0 && !state.extras.klr_stock_values.is_empty() {
            let counter = *state
                .extras
                .klr_stock_values
                .get("COUNTER")
                .ok_or_else(|| Error::Signal("COUNTER".into()))?;
            if Some(counter) != self.history.klr_counter_last {
                self.require_meb("create_capacitive_wheel_touch")?;
                sends.push(can_aux::touch(
                    &mut self.packer,
                    can_aux::Touch {
                        stock: &state.extras.klr_stock_values,
                        active: cc.get_lat_active(),
                        bus: 2,
                    },
                )?);
                sends.push(can_aux::touch(
                    &mut self.packer,
                    can_aux::Touch {
                        stock: &state.extras.klr_stock_values,
                        active: cc.get_lat_active(),
                        bus: 0,
                    },
                )?);
            }
            self.history.klr_counter_last = Some(counter);
        }
        if self.config.flags & STOCK_EA_PRESENT != 0
            && !state.extras.ea_hud_stock_values.is_empty()
            && self.history.frame.is_multiple_of(2)
        {
            let blink = state.extras.left_blinker_active || state.extras.right_blinker_active;
            self.require_meb("create_blinker_control")?;
            sends.push(can_aux::blinker(
                &mut self.packer,
                can_aux::Blinker {
                    hud: &state.extras.ea_hud_stock_values,
                    control: &state.extras.ea_control_stock_values,
                    left: cc.get_left_blinker() && !blink,
                    right: cc.get_right_blinker() && !blink,
                    hide: cc.get_lat_active(),
                },
            )?);
        }
        if self.config.longitudinal && self.history.frame.is_multiple_of(2) {
            self.longitudinal(state, cc, &mut sends)?;
        }
        self.hud(state, cc, &mut sends)?;
        let gra = state
            .extras
            .gra_stock_values
            .as_ref()
            .ok_or(Error::Stock("gra_stock_values"))?;
        let counter = *gra
            .get("COUNTER")
            .ok_or_else(|| Error::Signal("COUNTER".into()))?;
        let cruise = cc.get_cruise_control()?;
        if self.config.pcm
            && Some(counter) != self.history.gra_acc_counter_last
            && (cruise.get_cancel() || cruise.get_resume())
        {
            sends.push(can_buttons::buttons(
                &mut self.packer,
                can_buttons::Buttons {
                    stock: gra,
                    cancel: cruise.get_cancel(),
                    resume: cruise.get_resume(),
                    bus: self.config.external_bus(),
                    family: self.config.family,
                },
            )?);
        }
        let mut message = Message::new_default();
        message.set_root(act)?;
        let mut result = message.get_root::<actuators::Builder>()?;
        match self.config.family {
            Family::Meb => result.set_curvature(float(self.history.apply_curvature_last)?),
            Family::Pq | Family::Mqb => {
                result.set_torque(float(f64::from(self.history.apply_torque_last) / 300.)?);
                result.set_torque_output_can(float(f64::from(self.history.apply_torque_last))?);
            }
        }
        self.history.gra_acc_counter_last = Some(counter);
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
    fn require_meb(&self, function: &'static str) -> Result<(), Error> {
        match self.config.family {
            Family::Meb => Ok(()),
            Family::Pq => Err(Error::SourceFunction {
                module: "pqcan",
                function,
            }),
            Family::Mqb => Err(Error::SourceFunction {
                module: "mqbcan",
                function,
            }),
        }
    }
}
