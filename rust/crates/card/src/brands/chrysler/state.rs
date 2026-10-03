use super::{Candidate, Error};
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
use std::sync::Arc;

pub(super) struct Config {
    pub candidate: Candidate,
    pub factor: f64,
    pub pcm: bool,
    pub blindspots: bool,
}
#[derive(Serialize)]
pub struct Extras {
    pub auto_high_beam: f64,
    pub button_counter: f64,
    pub lkas_car_model: f64,
    pub distance_button: f64,
    pub left_blinker_cnt: u32,
    pub right_blinker_cnt: u32,
    pub left_blinker_prev: bool,
    pub right_blinker_prev: bool,
}
impl Default for Extras {
    fn default() -> Self {
        Self {
            auto_high_beam: 0.,
            button_counter: 0.,
            lkas_car_model: -1.,
            distance_button: 0.,
            left_blinker_cnt: 0,
            right_blinker_cnt: 0,
            left_blinker_prev: false,
            right_blinker_prev: false,
        }
    }
}
pub struct State {
    pub pt: Parser,
    pub camera: Parser,
    pub out: Message,
    pub extras: Extras,
    pub logs: Vec<VehicleLog>,
    pub soft_hold: i16,
    pub is_metric: bool,
    config: Config,
    defs: Definitions,
    speed: SpeedFilter,
    cluster_seen: bool,
}
fn float(value: f64) -> Result<f32, Error> {
    value.to_f32().ok_or(Error::Numeric)
}
impl State {
    pub(super) fn new(dbc: Arc<Dbc>, config: Config, now: u64) -> Result<Self, Error> {
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
            config,
            defs,
            speed: SpeedFilter::new()?,
            cluster_seen: false,
        })
    }
    fn signal(&mut self, name: &str, key: &str, now: u64) -> Result<f64, Error> {
        Ok(self.pt.signal_lazy(name, key, now)?)
    }
    fn cruise_signal(&mut self, name: &str, key: &str, now: u64) -> Result<f64, Error> {
        if self.config.candidate.ram() {
            Ok(self.camera.signal_lazy(name, key, now)?)
        } else {
            self.signal(name, key, now)
        }
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
    fn blinkers(&mut self, left: bool, right: bool) -> [bool; 2] {
        if left {
            self.extras.right_blinker_cnt = 0;
            if !self.extras.left_blinker_prev {
                self.extras.left_blinker_cnt = 200;
            }
        }
        if right {
            self.extras.left_blinker_cnt = 0;
            if !self.extras.right_blinker_prev {
                self.extras.right_blinker_cnt = 200;
            }
        }
        self.extras.left_blinker_cnt = self.extras.left_blinker_cnt.saturating_sub(1);
        self.extras.right_blinker_cnt = self.extras.right_blinker_cnt.saturating_sub(1);
        self.extras.left_blinker_prev = left;
        self.extras.right_blinker_prev = right;
        [
            left || self.extras.left_blinker_cnt > 0,
            right || self.extras.right_blinker_cnt > 0,
        ]
    }
    pub fn update(&mut self, packets: &[Packet], now: u64) -> Result<Message, Error> {
        self.pt.update(packets)?;
        self.camera.update(packets)?;
        self.drain_logs();
        let mut message = Message::new_default();
        let mut ret = message.init_root::<car_state::Builder>();
        let previous = self.extras.distance_button;
        self.extras.distance_button = self.signal("CRUISE_BUTTONS", "ACC_Distance_Dec", now)?;
        let mut doors = false;
        for key in [
            "DOOR_OPEN_FL",
            "DOOR_OPEN_FR",
            "DOOR_OPEN_RL",
            "DOOR_OPEN_RR",
        ] {
            doors |= self.signal("BCM_1", key, now)? != 0.;
        }
        ret.set_door_open(doors);
        ret.set_seatbelt_unlatched(self.signal("ORC_1", "SEATBELT_DRIVER_UNLATCHED", now)? == 1.);
        ret.set_brake(0.);
        ret.set_brake_pressed(self.signal("ESP_1", "Brake_Pedal_State", now)? == 1.);
        let gas = float(self.signal("ECM_5", "Accelerator_Position", now)?)?;
        ret.set_gas(gas);
        ret.set_gas_pressed(f64::from(gas) > 1e-5);
        let raw = if self.config.candidate.ram() {
            self.signal("ESP_8", "Vehicle_Speed", now)? * (1. / 3.6)
        } else {
            (self.signal("SPEED_1", "SPEED_LEFT", now)?
                + self.signal("SPEED_1", "SPEED_RIGHT", now)?)
                / 2.
        };
        let raw = float(raw)?;
        ret.set_v_ego_raw(raw);
        let (gear_msg, gear_key) = if self.config.candidate.ram() {
            ("Transmission_Status", "Gear_State")
        } else {
            ("GEAR", "PRNDL")
        };
        let gear = self
            .signal(gear_msg, gear_key, now)?
            .to_i64()
            .ok_or(Error::Numeric)?;
        let address = self.pt.dbc.message(gear_msg)?.address;
        ret.set_gear_shifter(state_helpers::parse_gear(
            self.defs
                .get(&address)
                .and_then(|d| d.get(gear_key))
                .and_then(|d| d.get(&gear))
                .map(String::as_str),
        ));
        let [speed, accel] = self.speed.update(f64::from(raw));
        ret.set_v_ego(float(speed)?);
        ret.set_a_ego(float(accel)?);
        ret.set_standstill(f64::from(raw).partial_cmp(&0.001) != Some(std::cmp::Ordering::Greater));
        let wheels = [
            self.signal("ESP_6", "WHEEL_SPEED_FL", now)?,
            self.signal("ESP_6", "WHEEL_SPEED_FR", now)?,
            self.signal("ESP_6", "WHEEL_SPEED_RL", now)?,
            self.signal("ESP_6", "WHEEL_SPEED_RR", now)?,
        ];
        let wheels = state_helpers::wheel_speeds(wheels, self.config.factor, 1.);
        let mut w = ret.reborrow().init_wheel_speeds();
        w.set_fl(float(wheels[0])?);
        w.set_fr(float(wheels[1])?);
        w.set_rl(float(wheels[2])?);
        w.set_rr(float(wheels[3])?);
        let left = self.signal("STEERING_LEVERS", "TURN_SIGNALS", now)? == 1.;
        let right = self.signal("STEERING_LEVERS", "TURN_SIGNALS", now)? == 2.;
        let [left, right] = self.blinkers(left, right);
        ret.set_left_blinker(left);
        ret.set_right_blinker(right);
        ret.set_generic_toggle(self.signal("STEERING_LEVERS", "HIGH_BEAM_PRESSED", now)? == 1.);
        ret.set_steering_angle_deg(float(
            self.signal("STEERING", "STEERING_ANGLE", now)?
                + self.signal("STEERING", "STEERING_ANGLE_HP", now)?,
        )?);
        ret.set_steering_rate_deg(float(self.signal("STEERING", "STEERING_RATE", now)?)?);
        let torque = float(self.signal("EPS_2", "COLUMN_TORQUE", now)?)?;
        ret.set_steering_torque(torque);
        ret.set_steering_torque_eps(float(self.signal("EPS_2", "EPS_TORQUE_MOTOR", now)?)?);
        ret.set_steering_pressed(torque.abs() > 120.);
        let available = self.cruise_signal("DAS_3", "ACC_AVAILABLE", now)? == 1.;
        let enabled = self.cruise_signal("DAS_3", "ACC_ACTIVE", now)? == 1.;
        let cruise_speed =
            float(self.cruise_signal("DAS_4", "ACC_SET_SPEED_KPH", now)? * (1. / 3.6))?;
        let adaptive = self.cruise_signal("DAS_4", "ACC_STATE", now)?;
        let standstill = self.cruise_signal("DAS_3", "ACC_STANDSTILL", now)? == 1.;
        let mut c = ret.reborrow().init_cruise_state();
        c.set_available(available);
        c.set_enabled(enabled);
        c.set_speed(cruise_speed);
        c.set_non_adaptive(adaptive == 1. || adaptive == 2.);
        c.set_standstill(standstill);
        ret.set_acc_faulted(self.cruise_signal("DAS_3", "ACC_FAULTED", now)? != 0.);
        if self.config.candidate.ram() {
            self.extras.auto_high_beam =
                self.camera.signal_lazy("DAS_6", "AUTO_HIGH_BEAM_ON", now)?;
            ret.set_steer_fault_temporary(self.signal("EPS_3", "DASM_FAULT", now)? == 1.);
        } else {
            ret.set_steer_fault_temporary(self.signal("EPS_2", "LKAS_TEMPORARY_FAULT", now)? == 1.);
            ret.set_steer_fault_permanent(self.signal("EPS_2", "LKAS_STATE", now)? == 4.);
        }
        if self.config.blindspots {
            ret.set_left_blindspot(self.signal("BSM_1", "LEFT_STATUS", now)? == 1.);
            ret.set_right_blindspot(self.signal("BSM_1", "RIGHT_STATUS", now)? == 1.);
        }
        self.extras.lkas_car_model = self.camera.signal_lazy("DAS_6", "CAR_MODEL", now)?;
        self.extras.button_counter = self.signal("CRUISE_BUTTONS", "COUNTER", now)?;
        let changes = if previous == self.extras.distance_button {
            Vec::new()
        } else {
            [(previous, false), (self.extras.distance_button, true)]
                .into_iter()
                .filter(|(value, _)| *value != 0.)
                .collect::<Vec<_>>()
        };
        let mut events = ret
            .reborrow()
            .init_button_events(u32::try_from(changes.len()).map_err(|_| Error::Numeric)?);
        for (index, (value, pressed)) in changes.iter().enumerate() {
            let mut event = events
                .reborrow()
                .get(u32::try_from(index).map_err(|_| Error::Numeric)?);
            event.set_type(if *value == 1. {
                Button::GapAdjustCruise
            } else {
                Button::Unknown
            });
            event.set_pressed(*pressed);
        }
        ret.set_can_valid(self.pt.can_valid() && self.camera.can_valid());
        ret.set_can_timeout(self.pt.bus_timeout() || self.camera.bus_timeout());
        if ret.reborrow_as_reader().get_v_ego_cluster() == 0. && !self.cluster_seen {
            ret.set_v_ego_cluster(ret.reborrow_as_reader().get_v_ego());
        } else {
            self.cluster_seen = true;
        }
        ret.reborrow()
            .get_cruise_state()?
            .set_speed_cluster(cruise_speed);
        ret.set_button_enable(state_helpers::button_enable(
            self.config.pcm,
            ret.reborrow_as_reader().get_button_events()?,
        )?);
        self.out.set_root(ret.into_reader())?;
        self.drain_logs();
        Ok(message)
    }
}
