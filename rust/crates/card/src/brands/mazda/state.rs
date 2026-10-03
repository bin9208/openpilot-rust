use super::Error;
use crate::{
    core::{Message, VehicleLog},
    query::DiagnosticLevel,
    state_helpers::{self, SpeedFilter},
};
use num_traits::ToPrimitive;
use openpilot_can::{
    dbc::{Dbc, Definitions},
    parser::Parser,
    Packet,
};
use openpilot_cereal::car_capnp::car_state::{self, button_event::Type as Button};
use serde::Serialize;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Default, Serialize)]
pub struct Extras {
    pub crz_btns_counter: f64,
    pub acc_active_last: bool,
    pub low_speed_alert: bool,
    pub lkas_allowed_speed: bool,
    pub lkas_disabled: bool,
    pub prev_distance_button: f64,
    pub distance_button: f64,
    pub prev_cruise_buttons: i32,
    pub cruise_buttons: i32,
    pub lkas_previously_enabled: bool,
    pub lkas_enabled: bool,
    pub left_blinker_cnt: u32,
    pub right_blinker_cnt: u32,
    pub cam_lkas: BTreeMap<String, f64>,
    pub cam_laneinfo: BTreeMap<String, f64>,
}
pub struct State {
    pub pt: Parser,
    pub camera: Parser,
    pub out: Message,
    pub extras: Extras,
    pub logs: Vec<VehicleLog>,
    pub soft_hold: i16,
    pub is_metric: bool,
    speed: SpeedFilter,
    defs: Definitions,
    factor: f64,
    min_steer_speed: f32,
    pcm: bool,
    cluster_seen: bool,
}
fn f32_value(value: f64) -> Result<f32, Error> {
    value.to_f32().ok_or(Error::Numeric)
}
fn snapshot(parser: &mut Parser, name: &str, now: u64) -> Result<BTreeMap<String, f64>, Error> {
    let names = parser
        .dbc
        .message(name)?
        .signals
        .iter()
        .map(|s| s.name.clone())
        .collect::<Vec<_>>();
    names
        .into_iter()
        .map(|key| {
            let value = parser.signal_lazy(name, &key, now)?;
            Ok((key, value))
        })
        .collect()
}
fn changes(current: i32, previous: i32, distance: bool) -> Vec<(Button, bool)> {
    if current == previous {
        return Vec::new();
    }
    [(previous, false), (current, true)]
        .into_iter()
        .filter(|(v, _)| *v != 0)
        .map(|(value, pressed)| {
            let button = match (distance, value) {
                (true, 1) => Button::GapAdjustCruise,
                (false, 1) => Button::AccelCruise,
                (false, 2) => Button::DecelCruise,
                (false, 3) => Button::ResumeCruise,
                (false, 4) => Button::Cancel,
                _ => Button::Unknown,
            };
            (button, pressed)
        })
        .collect()
}
impl State {
    pub fn new(
        dbc: Arc<Dbc>,
        factor: f64,
        min_steer_speed: f32,
        pcm: bool,
        now: u64,
    ) -> Result<Self, Error> {
        let defs = dbc.definitions()?;
        let mut out = Message::new_default();
        out.init_root::<car_state::Builder>();
        Ok(Self {
            pt: Parser::new(Arc::clone(&dbc), 0, now),
            camera: Parser::new(dbc, 2, now),
            out,
            extras: Extras::default(),
            logs: Vec::new(),
            soft_hold: 0,
            is_metric: true,
            speed: SpeedFilter::new()?,
            defs,
            factor,
            min_steer_speed,
            pcm,
            cluster_seen: false,
        })
    }
    fn signal(&mut self, name: &str, key: &str, now: u64) -> Result<f64, Error> {
        Ok(self.pt.signal_lazy(name, key, now)?)
    }
    fn drain_logs(&mut self) {
        for parser in [&mut self.pt, &mut self.camera] {
            self.logs.extend(
                std::mem::take(&mut parser.diagnostics)
                    .into_iter()
                    .map(|d| VehicleLog {
                        level: DiagnosticLevel::Warning,
                        message: d.message,
                    }),
            );
        }
    }
    pub fn update(&mut self, packets: &[Packet], now: u64) -> Result<Message, Error> {
        self.pt.update(packets)?;
        self.camera.update(packets)?;
        self.drain_logs();
        let mut message = Message::new_default();
        let mut ret = message.init_root::<car_state::Builder>();
        self.extras.prev_distance_button = self.extras.distance_button;
        self.extras.distance_button = self.signal("CRZ_BTNS", "DISTANCE_LESS", now)?;
        self.extras.prev_cruise_buttons = self.extras.cruise_buttons;
        self.extras.cruise_buttons = if self.signal("CRZ_BTNS", "SET_P", now)? != 0. {
            1
        } else if self.signal("CRZ_BTNS", "SET_M", now)? != 0. {
            2
        } else if self.signal("CRZ_BTNS", "RES", now)? != 0. {
            3
        } else {
            0
        };
        let mut values = [0.; 4];
        for (value, key) in values.iter_mut().zip(["FL", "FR", "RL", "RR"]) {
            *value = self.signal("WHEEL_SPEEDS", key, now)?;
        }
        let values = state_helpers::wheel_speeds(values, self.factor, 1. / 3.6).map(f32_value);
        let [fl, fr, rl, rr] = [
            values[0].as_ref().copied().map_err(|_| Error::Numeric)?,
            values[1].as_ref().copied().map_err(|_| Error::Numeric)?,
            values[2].as_ref().copied().map_err(|_| Error::Numeric)?,
            values[3].as_ref().copied().map_err(|_| Error::Numeric)?,
        ];
        let mut wheels = ret.reborrow().init_wheel_speeds();
        wheels.set_fl(fl);
        wheels.set_fr(fr);
        wheels.set_rl(rl);
        wheels.set_rr(rr);
        let raw = f32_value((f64::from(fl) + f64::from(fr) + f64::from(rl) + f64::from(rr)) / 4.)?;
        ret.set_v_ego_raw(raw);
        let [speed, accel] = self.speed.update(f64::from(raw));
        ret.set_v_ego(f32_value(speed)?);
        ret.set_a_ego(f32_value(accel)?);
        let kph = self.signal("ENGINE_DATA", "SPEED", now)?;
        ret.set_standstill(kph <= 0.1);
        let gear = self
            .signal("GEAR", "GEAR", now)?
            .to_i64()
            .ok_or(Error::Numeric)?;
        let address = self.pt.dbc.message("GEAR")?.address;
        ret.set_gear_shifter(state_helpers::parse_gear(
            self.defs
                .get(&address)
                .and_then(|d| d.get("GEAR"))
                .and_then(|d| d.get(&gear))
                .map(String::as_str),
        ));
        ret.set_gear_step(
            self.signal("GEAR", "GEAR_BOX", now)?
                .to_i16()
                .ok_or(Error::Numeric)?,
        );
        ret.set_generic_toggle(self.signal("BLINK_INFO", "HIGH_BEAMS", now)? != 0.);
        ret.set_left_blindspot(self.signal("BSM", "LEFT_BS_STATUS", now)? != 0.);
        ret.set_right_blindspot(self.signal("BSM", "RIGHT_BS_STATUS", now)? != 0.);
        let left = self.signal("BLINK_INFO", "LEFT_BLINK", now)? == 1.;
        let right = self.signal("BLINK_INFO", "RIGHT_BLINK", now)? == 1.;
        self.extras.left_blinker_cnt = if left {
            40
        } else {
            self.extras.left_blinker_cnt.saturating_sub(1)
        };
        self.extras.right_blinker_cnt = if right {
            40
        } else {
            self.extras.right_blinker_cnt.saturating_sub(1)
        };
        ret.set_left_blinker(self.extras.left_blinker_cnt > 0);
        ret.set_right_blinker(self.extras.right_blinker_cnt > 0);
        ret.set_steering_angle_deg(f32_value(self.signal("STEER", "STEER_ANGLE", now)?)?);
        let torque = f32_value(self.signal("STEER_TORQUE", "STEER_TORQUE_SENSOR", now)?)?;
        ret.set_steering_torque(torque);
        ret.set_steering_pressed(torque.abs() > 15.);
        ret.set_steering_torque_eps(f32_value(self.signal(
            "STEER_TORQUE",
            "STEER_TORQUE_MOTOR",
            now,
        )?)?);
        ret.set_steering_rate_deg(f32_value(self.signal(
            "STEER_RATE",
            "STEER_ANGLE_RATE",
            now,
        )?)?);
        ret.set_brake_pressed(self.signal("PEDALS", "BRAKE_ON", now)? == 1.);
        ret.set_brake(f32_value(self.signal("BRAKE", "BRAKE_PRESSURE", now)?)?);
        ret.set_seatbelt_unlatched(self.signal("SEATBELT", "DRIVER_SEATBELT", now)? == 0.);
        let mut doors = false;
        for key in ["FL", "FR", "BL", "BR"] {
            doors |= self.signal("DOORS", key, now)? != 0.;
        }
        ret.set_door_open(doors);
        let gas = f32_value(self.signal("ENGINE_DATA", "PEDAL_GAS", now)?)?;
        ret.set_gas(gas);
        ret.set_gas_pressed(gas > 0.);
        let blocked = self.signal("STEER_RATE", "LKAS_BLOCK", now)? == 1.;
        if self.min_steer_speed > 0. {
            if kph > 52. && !blocked {
                self.extras.lkas_allowed_speed = true;
            } else if kph < 45. {
                self.extras.lkas_allowed_speed = false;
            }
        } else {
            self.extras.lkas_allowed_speed = true;
        }
        let available = self.signal("CRZ_CTRL", "CRZ_AVAILABLE", now)? == 1.;
        let enabled = self.signal("CRZ_CTRL", "CRZ_ACTIVE", now)? == 1.;
        let stopped = self.signal("PEDALS", "STANDSTILL", now)? == 1.;
        let cruise = f32_value(self.signal("CRZ_EVENTS", "CRZ_SPEED", now)? * (1. / 3.6))?;
        let mut crz = ret.reborrow().init_cruise_state();
        crz.set_available(available);
        crz.set_enabled(enabled);
        crz.set_standstill(stopped);
        crz.set_speed(cruise);
        let disabled = self.camera.signal_lazy("CAM_LANEINFO", "LANE_LINES", now)? == 0.;
        ret.set_invalid_lkas_setting(disabled);
        if enabled {
            self.extras.low_speed_alert =
                !self.extras.lkas_allowed_speed && self.extras.acc_active_last;
        }
        ret.set_low_speed_alert(self.extras.low_speed_alert);
        ret.set_steer_fault_temporary(self.extras.lkas_allowed_speed && blocked);
        self.extras.acc_active_last = enabled;
        self.extras.crz_btns_counter = self.signal("CRZ_BTNS", "CTR", now)?;
        self.extras.lkas_disabled = disabled;
        self.extras.cam_lkas = snapshot(&mut self.camera, "CAM_LKAS", now)?;
        self.extras.cam_laneinfo = snapshot(&mut self.camera, "CAM_LANEINFO", now)?;
        ret.set_steer_fault_permanent(self.camera.signal("CAM_LKAS", "ERR_BIT_1")? == 1.);
        self.extras.lkas_previously_enabled = self.extras.lkas_enabled;
        self.extras.lkas_enabled = !disabled;
        let mut buttons = changes(
            self.extras.cruise_buttons,
            self.extras.prev_cruise_buttons,
            false,
        );
        buttons.extend(changes(
            self.extras.distance_button.to_i32().ok_or(Error::Numeric)?,
            self.extras
                .prev_distance_button
                .to_i32()
                .ok_or(Error::Numeric)?,
            true,
        ));
        let mut events = ret
            .reborrow()
            .init_button_events(u32::try_from(buttons.len()).map_err(|_| Error::Numeric)?);
        for (index, (kind, pressed)) in buttons.iter().enumerate() {
            let mut event = events
                .reborrow()
                .get(u32::try_from(index).map_err(|_| Error::Numeric)?);
            event.set_type(*kind);
            event.set_pressed(*pressed);
        }
        ret.set_can_valid(self.pt.can_valid() && self.camera.can_valid());
        ret.set_can_timeout(self.pt.bus_timeout() || self.camera.bus_timeout());
        if ret.reborrow_as_reader().get_v_ego_cluster() == 0. && !self.cluster_seen {
            ret.set_v_ego_cluster(ret.reborrow_as_reader().get_v_ego());
        } else {
            self.cluster_seen = true;
        }
        ret.reborrow().get_cruise_state()?.set_speed_cluster(cruise);
        ret.set_button_enable(state_helpers::button_enable(
            self.pcm,
            ret.reborrow_as_reader().get_button_events()?,
        )?);
        self.out.set_root(ret.into_reader())?;
        self.drain_logs();
        Ok(message)
    }
}
