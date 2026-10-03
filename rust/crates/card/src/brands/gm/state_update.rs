use super::{
    float, integer,
    state::{snapshot, State},
    Error,
};
use crate::{core::Message, state_helpers};
use openpilot_can::Packet;
use openpilot_cereal::car_capnp::car_state::{self, GearShifter};

impl State {
    pub fn update(&mut self, packets: &[Packet], now: u64) -> Result<Message, Error> {
        self.pt.update(packets)?;
        self.camera.update(packets)?;
        self.loopback.update(packets)?;
        self.drain_logs();
        let mut message = Message::new_default();
        let mut ret = message.init_root::<car_state::Builder>();
        let previous_buttons = self.extras.cruise_buttons;
        let previous_distance = self.extras.distance_button;
        self.extras.cruise_buttons = self.signal("ASCMSteeringButton", "ACCButtons", now)?;
        self.extras.distance_button = self.signal("ASCMSteeringButton", "DistanceButton", now)?;
        self.extras.buttons_counter = self.signal("ASCMSteeringButton", "RollingCounter", now)?;
        self.extras.pscm_status = Some(snapshot(&mut self.pt, "PSCMStatus", now)?);
        if (self.extras.cruise_buttons == 0. || self.extras.cruise_buttons == 1.)
            && self.extras.distance_button != 0.
        {
            self.extras.cruise_buttons = 7.;
        }
        if self.config.blindspots {
            ret.set_left_blindspot(self.signal("BCMBlindSpotMonitor", "LeftBSM", now)? == 1.);
            ret.set_right_blindspot(self.signal("BCMBlindSpotMonitor", "RightBSM", now)? == 1.);
        }
        let definition = self.loopback.dbc.message("ASCMLKASteeringCmd")?;
        let index = definition
            .signals
            .iter()
            .position(|s| s.name == "RollingCounter")
            .ok_or_else(|| Error::Signal("RollingCounter".into()))?;
        let state = self
            .loopback
            .states
            .get(&definition.address)
            .ok_or(Error::Stock("loopback"))?;
        self.extras.loopback_lka_steering_cmd_updated = !state.all_values[index].is_empty();
        if self.extras.loopback_lka_steering_cmd_updated {
            self.extras.loopback_lka_steering_cmd_ts_nanos = state
                .timestamps
                .back()
                .copied()
                .ok_or(Error::Stock("loopback timestamp"))?;
        }
        if self.config.camera && self.config.flags & 4 == 0 {
            self.extras.pt_lka_steering_cmd_counter =
                self.signal("ASCMLKASteeringCmd", "RollingCounter", now)?;
            self.extras.cam_lka_steering_cmd_counter =
                self.camera_signal("ASCMLKASteeringCmd", "RollingCounter", now)?;
        }
        let left = if self.signal("EBCMWheelSpdRear", "RLWheelDir", now)? == 2. {
            -1.
        } else {
            1.
        };
        let right = if self.signal("EBCMWheelSpdRear", "RRWheelDir", now)? == 2. {
            -1.
        } else {
            1.
        };
        let raw_wheels = [
            left * self.signal("EBCMWheelSpdFront", "FLWheelSpd", now)?,
            right * self.signal("EBCMWheelSpdFront", "FRWheelSpd", now)?,
            left * self.signal("EBCMWheelSpdRear", "RLWheelSpd", now)?,
            right * self.signal("EBCMWheelSpdRear", "RRWheelSpd", now)?,
        ];
        let values = state_helpers::wheel_speeds(raw_wheels, self.config.factor, 1. / 3.6);
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
        let raw = float((f64::from(fl) + f64::from(fr) + f64::from(rl) + f64::from(rr)) / 4.)?;
        ret.set_v_ego_raw(raw);
        let [speed, accel] = self.speed.update(f64::from(raw));
        ret.set_v_ego(float(speed)?);
        ret.set_a_ego(float(accel)?);
        let threshold = 10. * 0.0311 * (1. / 3.6);
        ret.set_standstill(f64::from(rl).abs() <= threshold && f64::from(rr).abs() <= threshold);
        let gear = if self.signal("ECMPRDNL2", "ManualMode", now)? == 1. {
            GearShifter::Manumatic
        } else {
            let raw = i64::from(integer(self.signal("ECMPRDNL2", "PRNDL2", now)?)?);
            let address = self.pt.dbc.message("ECMPRDNL2")?.address;
            state_helpers::parse_gear(
                self.defs
                    .get(&address)
                    .and_then(|v| v.get("PRNDL2"))
                    .and_then(|v| v.get(&raw))
                    .map(String::as_str),
            )
        };
        ret.set_gear_shifter(gear);
        let brake = float(if self.config.flags & 8 != 0 {
            self.signal("EBCMBrakePedalPosition", "BrakePedalPosition", now)? / 208.
        } else {
            self.signal("ECMAcceleratorPos", "BrakePedalPos", now)?
        })?;
        ret.set_brake(brake);
        ret.set_brake_pressed(if self.config.camera {
            self.signal("ECMEngineStatus", "BrakePressed", now)? != 0.
        } else {
            brake >= 10.
        });
        if self.config.direct {
            ret.set_regen_braking(self.signal("EBCMRegenPaddle", "RegenPaddle", now)? != 0.);
            self.extras.single_pedal_mode = gear == GearShifter::Low
                || self.signal("EVDriveMode", "SinglePedalModeActive", now)? == 1.;
        }
        let gas = float(if self.config.interceptor {
            (self.signal("GAS_SENSOR", "INTERCEPTOR_GAS", now)?
                + self.signal("GAS_SENSOR", "INTERCEPTOR_GAS2", now)?)
                / 2.
        } else {
            self.signal("AcceleratorPedal2", "AcceleratorPedal2", now)? / 254.
        })?;
        ret.set_gas(gas);
        ret.set_gas_pressed(
            f64::from(gas)
                > if self.config.interceptor {
                    if self.config.model.camera {
                        20.
                    } else {
                        4.
                    }
                } else {
                    1e-5
                },
        );
        ret.set_steering_angle_deg(float(self.signal(
            "PSCMSteeringAngle",
            "SteeringWheelAngle",
            now,
        )?)?);
        ret.set_steering_rate_deg(float(self.signal(
            "PSCMSteeringAngle",
            "SteeringWheelRate",
            now,
        )?)?);
        let torque = float(self.signal("PSCMStatus", "LKADriverAppldTrq", now)?)?;
        ret.set_steering_torque(torque);
        ret.set_steering_pressed(torque.abs() > 1.);
        ret.set_steering_torque_eps(float(self.signal(
            "PSCMStatus",
            "LKATorqueDelivered",
            now,
        )?)?);
        let lkas = self.signal("PSCMStatus", "LKATorqueDeliveredStatus", now)?;
        self.extras.lkas_status = Some(lkas);
        ret.set_steer_fault_temporary(lkas == 2.);
        ret.set_steer_fault_permanent(lkas == 3.);
        let mut door = false;
        for key in [
            "FrontLeftDoor",
            "FrontRightDoor",
            "RearLeftDoor",
            "RearRightDoor",
        ] {
            if door {
                break;
            }
            door = self.signal("BCMDoorBeltStatus", key, now)? == 1.;
        }
        ret.set_door_open(door);
        ret.set_seatbelt_unlatched(self.signal("BCMDoorBeltStatus", "LeftSeatBelt", now)? == 0.);
        let turn = self.signal("BCMTurnSignals", "TurnSignals", now)?;
        ret.set_left_blinker(turn == 1.);
        ret.set_right_blinker(turn == 2.);
        ret.set_parking_brake(
            self.signal("BCMGeneralPlatformStatus", "ParkBrakeSwActive", now)? == 1.,
        );
        self.controls(&mut ret, now, previous_buttons, previous_distance)?;
        self.finish(ret)?;
        Ok(message)
    }
}
