use super::Error;
use crate::{
    core::{Message, VehicleLog},
    query::DiagnosticLevel,
    state_helpers::{self, SpeedFilter},
};
use num_traits::ToPrimitive;
use openpilot_can::{dbc::Dbc, parser::Parser, Packet};
use openpilot_cereal::car_capnp::car_state::{self, GearShifter};
use serde::Serialize;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Serialize)]
pub struct Extras {
    pub last_speed: i64,
    pub steering_pressed_cnt: u32,
    pub acm_lka_hba_cmd: Option<BTreeMap<String, f64>>,
    pub sccm_wheel_touch: Option<BTreeMap<String, f64>>,
    pub vdm_adas_status: Option<BTreeMap<String, f64>>,
}
impl Default for Extras {
    fn default() -> Self {
        Self {
            last_speed: 30,
            steering_pressed_cnt: 0,
            acm_lka_hba_cmd: None,
            sccm_wheel_touch: None,
            vdm_adas_status: None,
        }
    }
}
pub struct State {
    pub pt: Parser,
    pub adas: Parser,
    pub camera: Parser,
    pub out: Message,
    pub extras: Extras,
    pub logs: Vec<VehicleLog>,
    pub soft_hold: i16,
    pub is_metric: bool,
    longitudinal: bool,
    pcm: bool,
    speed: SpeedFilter,
    cluster_seen: bool,
}
#[derive(Serialize)]
pub struct Snapshot {
    speed_filter: [f64; 2],
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
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            speed_filter: self.speed.state(),
            cluster_seen: self.cluster_seen,
        }
    }
    pub fn new(dbc: Arc<Dbc>, longitudinal: bool, pcm: bool, now: u64) -> Result<Self, Error> {
        let mut out = Message::new_default();
        out.init_root::<car_state::Builder>();
        Ok(Self {
            pt: Parser::new(Arc::clone(&dbc), 0, now),
            adas: Parser::new(Arc::clone(&dbc), 1, now),
            camera: Parser::new(dbc, 2, now),
            out,
            extras: Extras::default(),
            logs: Vec::new(),
            soft_hold: 0,
            is_metric: true,
            longitudinal,
            pcm,
            speed: SpeedFilter::new()?,
            cluster_seen: false,
        })
    }
    fn signal(&mut self, name: &str, key: &str, now: u64) -> Result<f64, Error> {
        Ok(self.pt.signal_lazy(name, key, now)?)
    }
    fn adas_signal(&mut self, name: &str, key: &str, now: u64) -> Result<f64, Error> {
        Ok(self.adas.signal_lazy(name, key, now)?)
    }
    fn camera_signal(&mut self, name: &str, key: &str, now: u64) -> Result<f64, Error> {
        Ok(self.camera.signal_lazy(name, key, now)?)
    }
    fn drain_logs(&mut self) {
        for parser in [&mut self.pt, &mut self.adas, &mut self.camera] {
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
        self.adas.update(packets)?;
        self.camera.update(packets)?;
        self.drain_logs();
        let mut message = Message::new_default();
        let mut ret = message.init_root::<car_state::Builder>();
        let raw = float(self.signal("ESP_Status", "ESP_Vehicle_Speed", now)? * (1. / 3.6))?;
        ret.set_v_ego_raw(raw);
        let [speed, accel] = self.speed.update(f64::from(raw));
        ret.set_v_ego(float(speed)?);
        ret.set_a_ego(float(accel)?);
        ret.set_standstill(f64::from(raw.abs()) < 0.01);
        let pedal = self.signal("VDM_PropStatus", "VDM_AcceleratorPedalPosition", now)?;
        ret.set_gas(float(pedal / 100.)?);
        ret.set_gas_pressed(pedal > 0.);
        ret.set_brake(float(self.signal("ESPiB3", "ESPiB3_pMC1", now)? / 250.)?);
        ret.set_brake_pressed(self.signal("iBESP2", "iBESP2_BrakePedalApplied", now)? == 1.);
        ret.set_steering_angle_deg(float(self.signal(
            "EPAS_AdasStatus",
            "EPAS_InternalSas",
            now,
        )?)?);
        ret.set_steering_rate_deg(float(self.signal(
            "EPAS_AdasStatus",
            "EPAS_SteeringAngleSpeed",
            now,
        )?)?);
        let torque = float(self.signal("EPAS_SystemStatus", "EPAS_TorsionBarTorque", now)?)?;
        ret.set_steering_torque(torque);
        self.extras.steering_pressed_cnt = if torque.abs() > 1. {
            self.extras.steering_pressed_cnt.saturating_add(1).min(6)
        } else {
            0
        };
        ret.set_steering_pressed(self.extras.steering_pressed_cnt > 5);
        ret.set_steer_fault_temporary(
            self.signal("EPAS_AdasStatus", "EPAS_EacErrorCode", now)? != 0.,
        );
        let limit = self
            .adas_signal("ACM_tsrCmd", "ACM_tsrSpdDisClsMain", now)?
            .to_i64()
            .ok_or(Error::Numeric)?
            .min(85);
        if limit != 0 {
            self.extras.last_speed = limit;
        }
        let enabled = self.camera_signal("ACM_Status", "ACM_FeatureStatus", now)? == 1.;
        let speed = if self.longitudinal {
            float(self.extras.last_speed.to_f64().ok_or(Error::Numeric)? * (1.609344 * (1. / 3.6)))?
        } else {
            -1.
        };
        let standstill = self.signal("VDM_AdasSts", "VDM_AdasAccelRequestAcknowledged", now)? == 1.;
        let mut cruise = ret.reborrow().init_cruise_state();
        cruise.set_enabled(enabled);
        cruise.set_speed(speed);
        cruise.set_available(true);
        cruise.set_standstill(standstill);
        let fault = self.camera_signal("ACM_Status", "ACM_FaultStatus", now)? == 1.
            || self.signal("VDM_AdasSts", "VDM_AdasFaultStatus", now)? == 3.;
        ret.set_acc_faulted(fault);
        let gear = self
            .signal("VDM_PropStatus", "VDM_Prndl_Status", now)?
            .to_i64()
            .ok_or(Error::Numeric)?;
        ret.set_gear_shifter(match gear {
            1 => GearShifter::Park,
            2 => GearShifter::Reverse,
            3 => GearShifter::Neutral,
            4 => GearShifter::Drive,
            _ => GearShifter::Unknown,
        });
        let mut door = false;
        for key in [
            "RearDriverDoor",
            "FrontPassengerDoor",
            "DriverDoor",
            "RearPassengerDoor",
        ] {
            door |= self.adas_signal("IndicatorLights", key, now)? != 2.;
        }
        ret.set_door_open(door);
        let left = self.adas_signal("IndicatorLights", "TurnLightLeft", now)?;
        let right = self.adas_signal("IndicatorLights", "TurnLightRight", now)?;
        ret.set_left_blinker(left == 1. || left == 2.);
        ret.set_right_blinker(right == 1. || right == 2.);
        ret.set_seatbelt_unlatched(
            self.signal("RCM_Status", "RCM_Status_IND_WARN_BELT_DRIVER", now)? != 0.,
        );
        ret.set_stock_aeb(self.camera_signal("ACM_AebRequest", "ACM_EnableRequest", now)? != 0.);
        self.extras.acm_lka_hba_cmd = Some(snapshot(&mut self.camera, "ACM_lkaHbaCmd", now)?);
        self.extras.sccm_wheel_touch = Some(snapshot(&mut self.pt, "SCCM_WheelTouch", now)?);
        self.extras.vdm_adas_status = Some(snapshot(&mut self.pt, "VDM_AdasSts", now)?);
        ret.set_can_valid(self.pt.can_valid() && self.adas.can_valid() && self.camera.can_valid());
        ret.set_can_timeout(
            self.pt.bus_timeout() || self.adas.bus_timeout() || self.camera.bus_timeout(),
        );
        if ret.reborrow_as_reader().get_v_ego_cluster() == 0. && !self.cluster_seen {
            ret.set_v_ego_cluster(ret.reborrow_as_reader().get_v_ego());
        } else {
            self.cluster_seen = true;
        }
        ret.reborrow().get_cruise_state()?.set_speed_cluster(speed);
        ret.set_button_enable(state_helpers::button_enable(
            self.pcm,
            ret.reborrow_as_reader().get_button_events()?,
        )?);
        self.out.set_root(ret.into_reader())?;
        self.drain_logs();
        Ok(message)
    }
}
