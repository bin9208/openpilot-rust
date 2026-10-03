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
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};

#[derive(Serialize)]
pub struct Extras {
    pub distance_button: f64,
    #[serde(rename = "steeringTorqueSamples")]
    pub steering_torque_samples: VecDeque<f64>,
    pub cruise_throttle_msg: BTreeMap<String, f64>,
    pub cancel_msg: BTreeMap<String, f64>,
    pub lkas_hud_msg: BTreeMap<String, f64>,
    pub lkas_hud_info_msg: BTreeMap<String, f64>,
}
pub struct State {
    pub pt: Parser,
    pub camera: Parser,
    pub adas: Parser,
    pub out: Message,
    pub extras: Extras,
    pub logs: Vec<VehicleLog>,
    pub soft_hold: i16,
    pub is_metric: bool,
    candidate: Candidate,
    factor: f64,
    pcm: bool,
    speed: SpeedFilter,
    defs: Definitions,
    cluster_seen: bool,
}
fn float(value: f64) -> Result<f32, Error> {
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
        .map(|key| Ok((key.clone(), parser.signal_lazy(name, &key, now)?)))
        .collect()
}
impl State {
    pub(super) fn new(
        dbc: Arc<Dbc>,
        candidate: Candidate,
        factor: f64,
        pcm: bool,
        now: u64,
    ) -> Result<Self, Error> {
        let defs = dbc.definitions()?;
        let mut out = Message::new_default();
        out.init_root::<car_state::Builder>();
        Ok(Self {
            pt: Parser::new(Arc::clone(&dbc), u8::from(candidate.altima()), now),
            camera: Parser::new(Arc::clone(&dbc), u8::from(!candidate.altima()), now),
            adas: Parser::new(dbc, 2, now),
            out,
            extras: Extras {
                distance_button: 0.,
                steering_torque_samples: VecDeque::from([0.; 12]),
                cruise_throttle_msg: BTreeMap::new(),
                cancel_msg: BTreeMap::new(),
                lkas_hud_msg: BTreeMap::new(),
                lkas_hud_info_msg: BTreeMap::new(),
            },
            logs: Vec::new(),
            soft_hold: 0,
            is_metric: true,
            candidate,
            factor,
            pcm,
            speed: SpeedFilter::new()?,
            defs,
            cluster_seen: false,
        })
    }
    fn signal(&mut self, name: &str, key: &str, now: u64) -> Result<f64, Error> {
        Ok(self.pt.signal_lazy(name, key, now)?)
    }
    fn adas_signal(&mut self, name: &str, key: &str, now: u64) -> Result<f64, Error> {
        Ok(self.adas.signal_lazy(name, key, now)?)
    }
    fn drain_logs(&mut self) {
        for parser in [&mut self.pt, &mut self.camera, &mut self.adas] {
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
        self.adas.update(packets)?;
        self.drain_logs();
        let mut message = Message::new_default();
        let mut ret = message.init_root::<car_state::Builder>();
        let previous = self.extras.distance_button;
        self.extras.distance_button =
            self.signal("CRUISE_THROTTLE", "FOLLOW_DISTANCE_BUTTON", now)?;
        let gas = float(self.signal(
            if self.candidate.leaf() {
                "CRUISE_THROTTLE"
            } else {
                "GAS_PEDAL"
            },
            "GAS_PEDAL",
            now,
        )?)?;
        ret.set_gas(gas);
        ret.set_gas_pressed(gas > 3.);
        ret.set_brake_pressed(
            self.signal(
                if self.candidate.leaf() {
                    "CRUISE_THROTTLE"
                } else {
                    "DOORS_LIGHTS"
                },
                "USER_BRAKE_PRESSED",
                now,
            )? != 0.,
        );
        let raw = [
            self.signal("WHEEL_SPEEDS_FRONT", "WHEEL_SPEED_FL", now)?,
            self.signal("WHEEL_SPEEDS_FRONT", "WHEEL_SPEED_FR", now)?,
            self.signal("WHEEL_SPEEDS_REAR", "WHEEL_SPEED_RL", now)?,
            self.signal("WHEEL_SPEEDS_REAR", "WHEEL_SPEED_RR", now)?,
        ];
        let values = state_helpers::wheel_speeds(raw, self.factor, 1. / 3.6);
        let [fl, fr, rl, rr] = [
            float(values[0])?,
            float(values[1])?,
            float(values[2])?,
            float(values[3])?,
        ];
        let mut wheels = ret.reborrow().init_wheel_speeds();
        wheels.set_fl(fl);
        wheels.set_fr(fr);
        wheels.set_rl(rl);
        wheels.set_rr(rr);
        ret.set_v_ego_raw(float((f64::from(rl) + f64::from(rr)) / 2.)?);
        let full_speed = (f64::from(fl) + f64::from(fr) + f64::from(rl) + f64::from(rr)) / 4.;
        let [speed, accel] = self.speed.update(full_speed);
        ret.set_v_ego(float(speed)?);
        ret.set_a_ego(float(accel)?);
        ret.set_standstill(raw[2] == 0. && raw[3] == 0.);
        let enabled = if self.candidate.altima() {
            self.signal("CRUISE_STATE", "CRUISE_ENABLED", now)?
        } else {
            self.adas_signal("CRUISE_STATE", "CRUISE_ENABLED", now)?
        } != 0.;
        let (seatbelt, available) = match self.candidate {
            Candidate::Rogue | Candidate::Xtrail => (
                self.signal("HUD", "SEATBELT_DRIVER_LATCHED", now)? == 0.,
                self.camera.signal_lazy("PRO_PILOT", "CRUISE_ON", now)? != 0.,
            ),
            Candidate::Leaf => (
                self.signal("SEATBELT", "SEATBELT_DRIVER_LATCHED", now)? == 0.,
                self.signal("CRUISE_THROTTLE", "CRUISE_AVAILABLE", now)? != 0.,
            ),
            Candidate::LeafIc => (
                self.signal("CANCEL_MSG", "CANCEL_SEATBELT", now)? == 1.,
                self.signal("CRUISE_THROTTLE", "CRUISE_AVAILABLE", now)? != 0.,
            ),
            Candidate::Altima => (
                self.signal("HUD", "SEATBELT_DRIVER_LATCHED", now)? == 0.,
                self.adas_signal("PRO_PILOT", "CRUISE_ON", now)? != 0.,
            ),
        };
        ret.set_seatbelt_unlatched(seatbelt);
        let speed_set = if self.candidate.altima() {
            self.signal("PROPILOT_HUD", "SET_SPEED", now)?
        } else {
            self.adas_signal("PROPILOT_HUD", "SET_SPEED", now)?
        };
        let mut cruise = ret.reborrow().init_cruise_state();
        cruise.set_enabled(enabled);
        cruise.set_available(available);
        if speed_set != 255. {
            let imperial = self.signal(
                if self.candidate.leaf() {
                    "HUD_SETTINGS"
                } else {
                    "HUD"
                },
                "SPEED_MPH",
                now,
            )? != 0.;
            let conversion = if imperial {
                1.609344 * (1. / 3.6)
            } else {
                1. / 3.6
            };
            cruise.set_speed(float(speed_set * conversion)?);
            cruise.set_speed_cluster(float((speed_set - 1.) * conversion)?);
        }
        let torque = if self.candidate.altima() {
            self.camera
                .signal_lazy("STEER_TORQUE_SENSOR", "STEER_TORQUE_DRIVER", now)?
        } else {
            self.signal("STEER_TORQUE_SENSOR", "STEER_TORQUE_DRIVER", now)?
        };
        let torque = float(torque)?;
        ret.set_steering_torque(torque);
        self.extras.steering_torque_samples.pop_front();
        self.extras
            .steering_torque_samples
            .push_back(f64::from(torque));
        ret.set_steering_pressed(
            (self.extras.steering_torque_samples.iter().sum::<f64>() / 12.).abs() > 1.,
        );
        ret.set_steering_angle_deg(float(self.signal(
            "STEER_ANGLE_SENSOR",
            "STEER_ANGLE",
            now,
        )?)?);
        ret.set_left_blinker(self.signal("LIGHTS", "LEFT_BLINKER", now)? != 0.);
        ret.set_right_blinker(self.signal("LIGHTS", "RIGHT_BLINKER", now)? != 0.);
        let mut doors = false;
        for key in [
            "DOOR_OPEN_RR",
            "DOOR_OPEN_RL",
            "DOOR_OPEN_FR",
            "DOOR_OPEN_FL",
        ] {
            doors |= self.signal("DOORS_LIGHTS", key, now)? != 0.;
        }
        ret.set_door_open(doors);
        ret.set_esp_disabled(self.signal("ESP", "ESP_DISABLED", now)? != 0.);
        let gear = self
            .signal("GEARBOX", "GEAR_SHIFTER", now)?
            .to_i64()
            .ok_or(Error::Numeric)?;
        let address = self.pt.dbc.message("GEARBOX")?.address;
        ret.set_gear_shifter(state_helpers::parse_gear(
            self.defs
                .get(&address)
                .and_then(|d| d.get("GEAR_SHIFTER"))
                .and_then(|d| d.get(&gear))
                .map(String::as_str),
        ));
        let lkas = if self.candidate.altima() {
            self.signal("LKAS_SETTINGS", "LKAS_ENABLED", now)?
        } else {
            self.adas_signal("LKAS_SETTINGS", "LKAS_ENABLED", now)?
        };
        ret.set_invalid_lkas_setting(lkas != 0.);
        self.extras.cruise_throttle_msg = snapshot(&mut self.pt, "CRUISE_THROTTLE", now)?;
        if self.candidate.leaf() {
            self.extras.cancel_msg = snapshot(&mut self.pt, "CANCEL_MSG", now)?;
        }
        if !self.candidate.altima() {
            self.extras.lkas_hud_msg = snapshot(&mut self.adas, "PROPILOT_HUD", now)?;
            self.extras.lkas_hud_info_msg = snapshot(&mut self.adas, "PROPILOT_HUD_INFO_MSG", now)?;
        }
        let changes = if self.extras.distance_button == previous {
            Vec::new()
        } else {
            [(previous, false), (self.extras.distance_button, true)]
                .into_iter()
                .filter(|(v, _)| *v != 0.)
                .collect::<Vec<_>>()
        };
        let mut buttons = ret
            .reborrow()
            .init_button_events(u32::try_from(changes.len()).map_err(|_| Error::Numeric)?);
        for (index, (value, pressed)) in changes.iter().enumerate() {
            let mut event = buttons
                .reborrow()
                .get(u32::try_from(index).map_err(|_| Error::Numeric)?);
            event.set_type(if *value == 1. {
                Button::GapAdjustCruise
            } else {
                Button::Unknown
            });
            event.set_pressed(*pressed);
        }
        ret.set_can_valid(self.pt.can_valid() && self.camera.can_valid() && self.adas.can_valid());
        ret.set_can_timeout(
            self.pt.bus_timeout() || self.camera.bus_timeout() || self.adas.bus_timeout(),
        );
        if ret.reborrow_as_reader().get_v_ego_cluster() == 0. && !self.cluster_seen {
            ret.set_v_ego_cluster(ret.reborrow_as_reader().get_v_ego());
        } else {
            self.cluster_seen = true;
        }
        let mut cruise = ret.reborrow().get_cruise_state()?;
        if cruise.reborrow_as_reader().get_speed_cluster() == 0. {
            cruise.set_speed_cluster(cruise.reborrow_as_reader().get_speed());
        }
        ret.set_button_enable(state_helpers::button_enable(
            self.pcm,
            ret.reborrow_as_reader().get_button_events()?,
        )?);
        self.out.set_root(ret.into_reader())?;
        self.drain_logs();
        Ok(message)
    }
}
