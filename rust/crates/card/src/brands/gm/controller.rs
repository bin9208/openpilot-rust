use super::{
    can, can_adas, controller_params::Limits, float, integer, model::Model, state::State, Error,
};
use crate::{
    brands::hyundai::parameters::setting_int,
    core::{ApplyInput, ApplyOutput, Message, VehicleLog},
    query::DiagnosticLevel,
};
use num_traits::ToPrimitive;
use openpilot_can::packer::Packer;
use openpilot_cereal::car_capnp::{
    car_control::actuators,
    car_params::{self, NetworkLocation},
    car_state,
};
use openpilot_params::Params;
use serde::Serialize;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Default, Serialize)]
pub struct Snapshot {
    pub start_time: f64,
    pub apply_torque_last: i32,
    pub apply_gas: i32,
    pub apply_brake: i32,
    pub apply_speed: i32,
    pub frame: u64,
    pub last_steer_frame: u64,
    pub last_button_frame: u64,
    pub cancel_counter: u64,
    pub pedal_steady: f64,
    pub lka_steering_cmd_counter: u64,
    pub lka_icon_status_last: (bool, bool),
    pub long_pitch: bool,
    pub use_ev_tables: bool,
    pub pitch: f64,
    pub accel_g: f64,
    #[serde(rename = "activateCruise_after_brake")]
    pub activate_cruise_after_brake: bool,
}
pub(super) struct Config {
    pub model: Model,
    pub camera: bool,
    pub longitudinal: bool,
    pub radar: bool,
    pub flags: u32,
    pub interceptor: bool,
    pub auto_resume: bool,
    pub stop_accel: f32,
    pub min_enable: f32,
}
pub struct Controller {
    pub snapshot: Snapshot,
    pub limits: Limits,
    pub logs: Vec<VehicleLog>,
    pub writes: Vec<(String, Vec<u8>)>,
    pub(super) pt: Packer,
    pub(super) object: Packer,
    pub(super) chassis: Packer,
    pub(super) settings: Arc<Params>,
    pub(super) config: Config,
}
impl Controller {
    pub fn new(
        pt: Packer,
        object: Packer,
        chassis: Packer,
        cp: car_params::Reader<'_>,
        settings: Arc<Params>,
    ) -> Result<Self, Error> {
        let model = Model::new(cp.get_car_fingerprint()?.to_str()?)?;
        let limits = Limits::new(&model);
        setting_int(&settings, "AutoEngage")?;
        setting_int(&settings, "UseLaneLineSpeed")?;
        let config = Config {
            model,
            camera: cp.get_network_location()? == NetworkLocation::FwdCamera,
            longitudinal: cp.get_openpilot_longitudinal_control(),
            radar: !cp.get_radar_unavailable(),
            flags: cp.get_flags(),
            interceptor: cp.get_enable_gas_interceptor_d_e_p_r_e_c_a_t_e_d(),
            auto_resume: cp.get_auto_resume_sng(),
            stop_accel: cp.get_stop_accel(),
            min_enable: cp.get_min_enable_speed(),
        };
        Ok(Self {
            snapshot: Snapshot::default(),
            limits,
            logs: Vec::new(),
            writes: Vec::new(),
            pt,
            object,
            chassis,
            settings,
            config,
        })
    }
    pub fn packer_counters(&self) -> [BTreeMap<u32, String>; 3] {
        [&self.pt, &self.object, &self.chassis].map(|p| {
            p.counters
                .iter()
                .map(|(a, c)| (*a, c.to_string()))
                .collect()
        })
    }
    pub fn apply(&mut self, state: &State, input: ApplyInput<'_>) -> Result<ApplyOutput, Error> {
        let frame = self.snapshot.frame;
        if frame.is_multiple_of(50) {
            for (key, target) in [
                ("CustomSteerMax", &mut self.limits.steer_max),
                ("CustomSteerDeltaUp", &mut self.limits.steer_delta_up),
                ("CustomSteerDeltaDown", &mut self.limits.steer_delta_down),
            ] {
                let value = setting_int(&self.settings, key)?;
                if value > 0 {
                    *target = value;
                }
            }
        }
        self.snapshot.long_pitch = self.settings.get_bool("LongPitch")?;
        self.snapshot.use_ev_tables = self.settings.get_bool("EVTable")?;
        let cc = input.control;
        let actuators = cc.get_actuators()?;
        let hud = cc.get_hud_control()?;
        let out = state.out.get_root_as_reader::<car_state::Reader>()?;
        let mut accel = f64::from(actuators.get_accel());
        let cruise_speed = if hud.get_set_speed() > 70. {
            0.
        } else {
            f64::from(hud.get_set_speed())
        };
        let mut sends = Vec::new();
        self.steering(state, cc, input.now_ns, &mut sends)?;
        if self.config.longitudinal {
            self.auto_resume(state, cc, &mut sends)?;
            if frame.is_multiple_of(4) {
                self.longitudinal(state, cc, &mut accel, cruise_speed, &mut sends)?;
            } else {
                accel += self.snapshot.accel_g;
            }
            if self.config.radar {
                if frame.is_multiple_of(10) {
                    let counter = i32::try_from((frame / 10) % 4).map_err(|_| Error::Numeric)?;
                    let ticks = ((frame.to_f64().ok_or(Error::Numeric)? * 0.01
                        - self.snapshot.start_time)
                        * 60.)
                        .to_u64()
                        .ok_or(Error::Numeric)?;
                    sends.push(can_adas::time_status(ticks, counter)?);
                    sends.push(can::message(
                        &mut self.object,
                        "ASCMHeadlight",
                        1,
                        &[("Always42", 66.), ("Always4", 4.)],
                    )?);
                }
                if frame.is_multiple_of(2) {
                    let counter = i32::try_from((frame / 2) % 4).map_err(|_| Error::Numeric)?;
                    sends.push(can_adas::steering_status(counter)?);
                    sends.push(can_adas::speed_status(
                        f64::from(out.get_v_ego()).abs(),
                        counter,
                    )?);
                }
            }
            if !self.config.camera && frame.is_multiple_of(100) {
                sends.extend(can_adas::keepalive());
            }
            if (self.config.flags & 1 != 0 || (self.config.flags & 2 != 0 && !cc.get_enabled()))
                && out.get_cruise_state()?.get_enabled()
                && self.button_ready()?
            {
                self.snapshot.last_button_frame = frame;
                sends.push(can::buttons(
                    &mut self.pt,
                    0,
                    (integer(state.extras.buttons_counter)? + 1).rem_euclid(4),
                    6,
                )?);
            }
        } else {
            self.snapshot.cancel_counter = if cc.get_cruise_control()?.get_cancel() {
                self.snapshot
                    .cancel_counter
                    .checked_add(1)
                    .ok_or(Error::Numeric)?
            } else {
                0
            };
            if self.button_ready()? && self.snapshot.cancel_counter > 10 {
                self.snapshot.last_button_frame = frame;
                sends.push(can::buttons(
                    &mut self.pt,
                    2,
                    (integer(state.extras.buttons_counter)? + 1).rem_euclid(4),
                    6,
                )?);
            }
        }
        if self.config.camera && frame.is_multiple_of(10) {
            sends.push(can::pscm(
                &mut self.pt,
                state
                    .extras
                    .pscm_status
                    .as_ref()
                    .ok_or(Error::Stock("pscm_status"))?,
            )?);
        }
        let mut result = Message::new_default();
        result.set_root(actuators)?;
        let mut a = result.get_root::<actuators::Builder>()?;
        a.set_accel(float(accel)?);
        a.set_torque(float(
            f64::from(self.snapshot.apply_torque_last) / f64::from(self.limits.steer_max),
        )?);
        a.set_torque_output_can(float(f64::from(self.snapshot.apply_torque_last))?);
        a.set_gas(float(f64::from(self.snapshot.apply_gas))?);
        a.set_brake(float(f64::from(self.snapshot.apply_brake))?);
        a.set_speed(float(f64::from(self.snapshot.apply_speed))?);
        self.snapshot.frame = frame.checked_add(1).ok_or(Error::Numeric)?;
        for packer in [&mut self.pt, &mut self.object, &mut self.chassis] {
            self.logs.extend(
                std::mem::take(&mut packer.diagnostics)
                    .into_iter()
                    .map(|d| VehicleLog {
                        level: DiagnosticLevel::Error,
                        message: d.message,
                    }),
            );
        }
        Ok(ApplyOutput {
            actuators: result,
            can: sends,
        })
    }
    pub(super) fn button_ready(&self) -> Result<bool, Error> {
        Ok((self.snapshot.frame - self.snapshot.last_button_frame)
            .to_f64()
            .ok_or(Error::Numeric)?
            * 0.01
            > 0.04)
    }
}
