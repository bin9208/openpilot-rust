use super::Error;
use crate::{
    core::{Message, VehicleLog},
    query::DiagnosticLevel,
    state_helpers::SpeedFilter,
};
use num_traits::ToPrimitive;
use openpilot_can::{dbc::Dbc, parser::Parser, Packet};
use openpilot_cereal::car_capnp::car_state::{self, GearShifter};
use serde::Serialize;
use std::sync::Arc;

pub struct State {
    pub main: Parser,
    pub adas: Parser,
    pub camera: Parser,
    pub out: Message,
    pub eps_active: Option<bool>,
    pub steering_pressed_count: u32,
    pub soft_hold: i16,
    pub logs: Vec<VehicleLog>,
    factor: f64,
    speed: SpeedFilter,
    cluster_seen: bool,
}
#[derive(Serialize)]
pub struct Snapshot {
    speed_filter: [f64; 2],
    cluster_seen: bool,
    pub eps_active: Option<bool>,
    steering_pressed_count: u32,
}
pub(super) fn float(value: f64) -> Result<f32, Error> {
    value.to_f32().ok_or(Error::Numeric)
}
impl State {
    pub fn new(dbc: Arc<Dbc>, factor: f64, now: u64) -> Result<Self, Error> {
        let mut out = Message::new_default();
        out.init_root::<car_state::Builder>();
        Ok(Self {
            main: Parser::new(Arc::clone(&dbc), 0, now),
            adas: Parser::new(Arc::clone(&dbc), 1, now),
            camera: Parser::new(dbc, 2, now),
            out,
            eps_active: None,
            steering_pressed_count: 0,
            soft_hold: 0,
            logs: Vec::new(),
            factor,
            speed: SpeedFilter::new()?,
            cluster_seen: false,
        })
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            speed_filter: self.speed.state(),
            cluster_seen: self.cluster_seen,
            eps_active: self.eps_active,
            steering_pressed_count: self.steering_pressed_count,
        }
    }
    fn signal(&mut self, bus: u8, message: &str, key: &str, now: u64) -> Result<f64, Error> {
        let parser = match bus {
            0 => &mut self.main,
            1 => &mut self.adas,
            _ => &mut self.camera,
        };
        Ok(parser.signal_lazy(message, key, now)?)
    }
    pub fn update(&mut self, packets: &[Packet], now: u64) -> Result<Message, Error> {
        for parser in [&mut self.main, &mut self.adas, &mut self.camera] {
            parser.update(packets)?;
            self.logs.extend(
                std::mem::take(&mut parser.diagnostics)
                    .into_iter()
                    .map(|d| VehicleLog {
                        level: DiagnosticLevel::Warning,
                        message: d.message,
                    }),
            );
        }
        let mut message = Message::new_default();
        let mut ret = message.init_root::<car_state::Builder>();
        let mut wheels = [0.; 4];
        for (value, key) in wheels.iter_mut().zip([
            "P263_VehV_VPsvValWhlFrtL",
            "P264_VehV_VPsvValWhlFrtR",
            "P265_VehV_VPsvValWhlBckL",
            "P266_VehV_VPsvValWhlBckR",
        ]) {
            *value = self.signal(0, "Dyn4_FRE", key, now)?;
        }
        // Official parse_wheel_speeds: sum raw CAN doubles before unit/factor and cereal rounding.
        let raw = float(wheels.into_iter().sum::<f64>() / 4. * (1. / 3.6) * self.factor)?;
        ret.set_v_ego_raw(raw);
        let [speed, acceleration] = self.speed.update(f64::from(raw));
        ret.set_v_ego(float(speed)?);
        ret.set_a_ego(float(acceleration)?);
        ret.set_yaw_rate(float(
            self.signal(1, "HS2_DYN_UCF_MDD_32D", "VITESSE_LACET_BRUTE", now)?
                * (std::f64::consts::PI / 180.),
        )?);
        let standstill = self.signal(1, "HS2_DYN_UCF_MDD_32D", "VEHICLE_STANDSTILL", now)? != 0.;
        ret.set_standstill(standstill);
        ret.set_gas_pressed(self.signal(0, "Dyn_CMM", "P002_Com_rAPP", now)? > 0.);
        ret.set_brake_pressed(self.signal(2, "Dat_BSI", "P013_MainBrake", now)? != 0.);
        ret.set_parking_brake(self.signal(0, "Dyn_EasyMove", "P337_Com_stPrkBrk", now)? == 1.);
        ret.set_steering_angle_deg(float(self.signal(0, "STEERING_ALT", "ANGLE", now)?)?);
        let rate = self.signal(0, "STEERING_ALT", "RATE", now)?
            * (2. * self.signal(0, "STEERING_ALT", "RATE_SIGN", now)? - 1.);
        ret.set_steering_rate_deg(float(rate)?);
        let torque = float(self.signal(0, "STEERING", "DRIVER_TORQUE", now)?)?;
        ret.set_steering_torque(torque);
        ret.set_steering_torque_eps(float(self.signal(0, "IS_DAT_DIRA", "EPS_TORQUE", now)?)?);
        self.steering_pressed_count = if torque.abs() > 5. {
            (self.steering_pressed_count + 1).min(6)
        } else {
            0
        };
        ret.set_steering_pressed(self.steering_pressed_count > 5);
        self.eps_active = Some(self.signal(0, "IS_DAT_DIRA", "EPS_STATE_LKA", now)? == 3.);
        let mut cruise = ret.reborrow().init_cruise_state();
        cruise.set_speed(float(
            self.signal(1, "HS2_DAT_MDD_CMD_452", "SPEED_SETPOINT", now)? * (1. / 3.6),
        )?);
        cruise.set_enabled(
            self.signal(1, "HS2_DAT_MDD_CMD_452", "RVV_ACC_ACTIVATION_REQ", now)? == 1.,
        );
        cruise.set_available(self.signal(1, "HS2_DYN1_MDD_ETAT_2B6", "ACC_STATUS", now)? > 2.);
        cruise.set_non_adaptive(
            self.signal(
                1,
                "HS2_DAT_MDD_CMD_452",
                "LONGITUDINAL_REGULATION_TYPE",
                now,
            )? != 3.,
        );
        cruise.set_standstill(standstill);
        ret.set_acc_faulted(
            self.signal(
                1,
                "HS2_DYN_UCF_MDD_32D",
                "ACC_ETAT_DECEL_OR_ESP_STATUS",
                now,
            )? == 3.,
        );
        ret.set_gear_shifter(
            if self.signal(2, "Dat_BSI", "P103_Com_bRevGear", now)? != 0. {
                GearShifter::Reverse
            } else {
                GearShifter::Drive
            },
        );
        let blinker = self.signal(2, "HS2_DAT7_BSI_612", "CDE_CLG_ET_HDC", now)?;
        ret.set_left_blinker(blinker == 1.);
        ret.set_right_blinker(blinker == 2.);
        let driver_door = self.signal(2, "Dat_BSI", "DRIVER_DOOR", now)? != 0.;
        let passenger_door = self.signal(2, "Dat_BSI", "PASSENGER_DOOR", now)? != 0.;
        ret.set_door_open(driver_door || passenger_door);
        ret.set_seatbelt_unlatched(self.signal(2, "RESTRAINTS", "DRIVER_SEATBELT", now)? != 2.);
        // Shared CarInterfaceBase wrapper; keep parser order and short-circuit can_valid side effects.
        ret.set_can_valid(
            self.main.can_valid() && self.adas.can_valid() && self.camera.can_valid(),
        );
        ret.set_can_timeout(
            self.main.bus_timeout() || self.adas.bus_timeout() || self.camera.bus_timeout(),
        );
        // can_valid itself emits diagnostics; expose those in this update, like the source logger.
        for parser in [&mut self.main, &mut self.adas, &mut self.camera] {
            self.logs.extend(
                std::mem::take(&mut parser.diagnostics)
                    .into_iter()
                    .map(|d| VehicleLog {
                        level: DiagnosticLevel::Warning,
                        message: d.message,
                    }),
            );
        }
        if ret.reborrow_as_reader().get_v_ego_cluster() == 0. && !self.cluster_seen {
            ret.set_v_ego_cluster(ret.reborrow_as_reader().get_v_ego());
        } else {
            self.cluster_seen = true;
        }
        // vEgoCluster is always unset by the PSA state; cluster_seen consequently stays false.
        let speed = ret.reborrow_as_reader().get_cruise_state()?.get_speed();
        ret.reborrow().get_cruise_state()?.set_speed_cluster(speed);
        self.out.set_root(ret.into_reader())?;
        Ok(message)
    }
}
