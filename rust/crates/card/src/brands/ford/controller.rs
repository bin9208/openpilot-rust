use super::{
    can, can_ui,
    limits::curvature_limit,
    state::{float, State},
    Error,
};
use crate::core::{ApplyInput, ApplyOutput, Message, VehicleLog};
use openpilot_can::packer::Packer;
use openpilot_cereal::car_capnp::{
    car_control::{actuators, h_u_d_control::VisualAlert},
    car_state,
};
use openpilot_control_policy::math::{clip, interp, maximum};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Default, Serialize)]
pub struct Snapshot {
    pub frame: u64,
    pub apply_curvature_last: f64,
    pub accel: f64,
    pub gas: f64,
    pub brake_request: bool,
    pub main_on_last: bool,
    pub lkas_enabled_last: bool,
    pub steer_alert_last: bool,
    pub lead_distance_bars_last: Option<i8>,
    pub distance_bar_frame: u64,
}
pub struct Controller {
    pub snapshot: Snapshot,
    pub logs: Vec<VehicleLog>,
    packer: Packer,
    main: u8,
    canfd: bool,
    longitudinal: bool,
}
impl Controller {
    pub fn new(packer: Packer, main: u8, canfd: bool, longitudinal: bool) -> Self {
        Self {
            snapshot: Snapshot::default(),
            logs: Vec::new(),
            packer,
            main,
            canfd,
            longitudinal,
        }
    }
    pub fn packer_counters(&self) -> BTreeMap<u32, String> {
        self.packer
            .counters
            .iter()
            .map(|(a, v)| (*a, v.to_string()))
            .collect()
    }
    pub fn apply(&mut self, state: &State, input: ApplyInput<'_>) -> Result<ApplyOutput, Error> {
        let cc = input.control;
        let actuators = cc.get_actuators()?;
        let hud = cc.get_hud_control()?;
        let out = state.out.get_root_as_reader::<car_state::Reader>()?;
        let main_on = out.get_cruise_state()?.get_available();
        let steer_alert = matches!(
            hud.get_visual_alert()?,
            VisualAlert::SteerRequired | VisualAlert::Ldw
        );
        let fcw = hud.get_visual_alert()? == VisualAlert::Fcw;
        let frame = self.snapshot.frame;
        let mut sends = Vec::new();
        let cruise = cc.get_cruise_control()?;
        let mut buttons = |bus, cancel, resume, toggle| {
            can::button(
                &mut self.packer,
                bus,
                state
                    .extras
                    .buttons_stock_values
                    .as_ref()
                    .ok_or(Error::Stock("buttons_stock_values"))?,
                cancel,
                resume,
                toggle,
            )
        };
        if cruise.get_cancel() {
            sends.push(buttons(self.main + 2, true, false, false)?);
            sends.push(buttons(self.main, true, false, false)?);
        } else if cruise.get_resume() && frame.is_multiple_of(5) {
            sends.push(buttons(self.main + 2, false, true, false)?);
            sends.push(buttons(self.main, false, true, false)?);
        } else {
            let stock = state
                .extras
                .acc_tja_status_stock_values
                .as_ref()
                .ok_or(Error::Stock("acc_tja_status_stock_values"))?;
            let status = stock
                .get("Tja_D_Stat")
                .ok_or_else(|| Error::Signal("Tja_D_Stat".into()))?;
            if *status != 0. && frame.is_multiple_of(20) {
                sends.push(buttons(self.main + 2, false, false, true)?);
            }
        }
        if frame.is_multiple_of(5) {
            let speed = f64::from(out.get_v_ego_raw());
            let current = -f64::from(out.get_yaw_rate()) / speed.max(0.1);
            self.snapshot.apply_curvature_last = curvature_limit(
                f64::from(actuators.get_curvature()),
                self.snapshot.apply_curvature_last,
                current,
                speed,
                cc.get_lat_active(),
                self.canfd,
            )?;
            let counter = if self.canfd {
                Some(u8::try_from((frame / 5) % 16).map_err(|_| Error::Numeric)?)
            } else {
                None
            };
            sends.push(can::lateral(
                &mut self.packer,
                self.main,
                cc.get_lat_active(),
                -self.snapshot.apply_curvature_last,
                counter,
            )?);
        }
        if frame.is_multiple_of(3) {
            sends.push(can::message(
                &mut self.packer,
                "Lane_Assist_Data1",
                self.main,
                &[],
            )?);
        }
        if self.longitudinal && frame.is_multiple_of(2) {
            let mut accel = f64::from(actuators.get_accel());
            let mut gas = accel;
            if cc.get_long_active() {
                let creep = interp(f64::from(out.get_v_ego()), &[1., 3.], &[0.6, 0.])?;
                accel -= interp(accel, &[0., 0.2], &[creep, 0.])?;
                accel = maximum(accel, self.snapshot.accel - (3.5 * 2. * 0.01));
            }
            accel = clip(accel, -3.5, 2.);
            gas = clip(gas, -3.5, 2.);
            if !cc.get_long_active() || gas < -0.5 {
                gas = -5.;
            }
            let orientation = cc.get_orientation_n_e_d()?;
            let pitch = if orientation.len() == 3 {
                let pitch = f64::from(orientation.get(1));
                if pitch.is_infinite() {
                    return Err(Error::Numeric);
                }
                pitch.sin() * 9.81
            } else {
                0.
            };
            if accel + pitch > 0.3 || !cc.get_long_active() {
                self.snapshot.brake_request = false;
            } else if accel + pitch < 0. {
                self.snapshot.brake_request = true;
            }
            sends.push(can::acceleration(
                &mut self.packer,
                self.main,
                can::AccCommand {
                    active: cc.get_long_active(),
                    gas,
                    accel,
                    stopping: actuators.get_long_control_state()?
                        == actuators::LongControlState::Stopping,
                    brake: self.snapshot.brake_request,
                },
            )?);
            self.snapshot.accel = accel;
            self.snapshot.gas = gas;
        }
        let mut send_ui = self.snapshot.main_on_last != main_on
            || self.snapshot.lkas_enabled_last != cc.get_lat_active()
            || self.snapshot.steer_alert_last != steer_alert;
        if frame.is_multiple_of(100) || send_ui {
            sends.push(can_ui::lkas_ui(
                &mut self.packer,
                self.main,
                state
                    .extras
                    .lkas_status_stock_values
                    .as_ref()
                    .ok_or(Error::Stock("lkas_status_stock_values"))?,
                can_ui::UiInput {
                    main_on,
                    enabled: cc.get_lat_active(),
                    alert: steer_alert,
                    hud,
                },
            )?);
        }
        if self.snapshot.lead_distance_bars_last != Some(hud.get_lead_distance_bars()) {
            send_ui = true;
            self.snapshot.distance_bar_frame = frame;
        }
        if frame.is_multiple_of(20) || send_ui {
            sends.push(can_ui::acc_ui(
                &mut self.packer,
                self.main,
                state
                    .extras
                    .acc_tja_status_stock_values
                    .as_ref()
                    .ok_or(Error::Stock("acc_tja_status_stock_values"))?,
                can_ui::AccUiInput {
                    ui: can_ui::UiInput {
                        main_on,
                        enabled: cc.get_lat_active(),
                        alert: fcw,
                        hud,
                    },
                    longitudinal: self.longitudinal,
                    standstill: out.get_cruise_state()?.get_standstill(),
                    show_distance: frame - self.snapshot.distance_bar_frame < 400,
                },
            )?);
        }
        self.snapshot.main_on_last = main_on;
        self.snapshot.lkas_enabled_last = cc.get_lat_active();
        self.snapshot.steer_alert_last = steer_alert;
        self.snapshot.lead_distance_bars_last = Some(hud.get_lead_distance_bars());
        let mut result = Message::new_default();
        result.set_root(actuators)?;
        let mut a = result.get_root::<actuators::Builder>()?;
        a.set_curvature(float(self.snapshot.apply_curvature_last)?);
        a.set_accel(float(self.snapshot.accel)?);
        a.set_gas(float(self.snapshot.gas)?);
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
            actuators: result,
            can: sends,
        })
    }
}
