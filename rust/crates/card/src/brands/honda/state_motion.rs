use super::{
    state::{float, State},
    Error,
};
use crate::state_helpers;
use num_traits::ToPrimitive;
use openpilot_cereal::car_capnp::{
    car_params::TransmissionType,
    car_state::{self, GearShifter},
};
use openpilot_control_policy::math::interp;

impl State {
    pub(super) fn motion(
        &mut self,
        ret: &mut car_state::Builder<'_>,
        now: u64,
    ) -> Result<(), Error> {
        ret.set_standstill(self.signal("ENGINE_DATA", "XMISSION_SPEED", now)? < 1e-5);
        let door = match self.config.candidate.as_str() {
            "HONDA_ACCORD"
            | "HONDA_CIVIC_BOSCH"
            | "HONDA_CIVIC_BOSCH_DIESEL"
            | "HONDA_CRV_HYBRID"
            | "HONDA_INSIGHT"
            | "ACURA_RDX_3G"
            | "HONDA_E"
            | "HONDA_CIVIC_2022"
            | "HONDA_HRV_3G" => self.signal("SCM_FEEDBACK", "DRIVERS_DOOR_OPEN", now)? != 0.,
            "HONDA_ODYSSEY_CHN" | "HONDA_FREED" | "HONDA_HRV" => {
                self.signal("SCM_BUTTONS", "DRIVERS_DOOR_OPEN", now)? != 0.
            }
            _ => {
                let mut open = false;
                for key in [
                    "DOOR_OPEN_FL",
                    "DOOR_OPEN_FR",
                    "DOOR_OPEN_RL",
                    "DOOR_OPEN_RR",
                ] {
                    open |= self.signal("DOORS_STATUS", key, now)? != 0.;
                }
                open
            }
        };
        ret.set_door_open(door);
        ret.set_seatbelt_unlatched(
            self.signal("SEATBELT_STATUS", "SEATBELT_DRIVER_LAMP", now)? != 0.
                || self.signal("SEATBELT_STATUS", "SEATBELT_DRIVER_LATCHED", now)? == 0.,
        );
        let status = self
            .signal("STEER_STATUS", "STEER_STATUS", now)?
            .to_i64()
            .ok_or(Error::Numeric)?;
        let address = self.pt.dbc.message("STEER_STATUS")?.address;
        let value = self
            .defs
            .get(&address)
            .and_then(|d| d.get("STEER_STATUS"))
            .and_then(|d| d.get(&status))
            .map_or("UNKNOWN", String::as_str);
        ret.set_steer_fault_permanent(!matches!(
            value,
            "NORMAL"
                | "NO_TORQUE_ALERT_1"
                | "NO_TORQUE_ALERT_2"
                | "LOW_SPEED_LOCKOUT"
                | "TMP_FAULT"
        ));
        ret.set_steer_fault_temporary(!matches!(
            value,
            "NORMAL" | "LOW_SPEED_LOCKOUT" | "NO_TORQUE_ALERT_2"
        ));
        if self.config.radarless() {
            ret.set_acc_faulted(self.signal("CRUISE_FAULT_STATUS", "CRUISE_FAULT", now)? != 0.);
        } else {
            if self.config.longitudinal {
                ret.set_acc_faulted(
                    self.signal("STANDSTILL", "BRAKE_ERROR_1", now)? != 0.
                        || self.signal("STANDSTILL", "BRAKE_ERROR_2", now)? != 0.,
                );
            }
            if !self.config.bosch() {
                ret.set_car_faulted_non_critical(
                    self.camera_signal("ACC_HUD", "ACC_PROBLEM", now)? != 0.
                        || self.camera_signal("LKAS_HUD", "LKAS_PROBLEM", now)? != 0.,
                );
            }
        }
        ret.set_esp_disabled(self.signal("VSA_STATUS", "ESP_DISABLED", now)? != 0.);
        let mut wheels = [0.; 4];
        for (index, key) in [
            "WHEEL_SPEED_FL",
            "WHEEL_SPEED_FR",
            "WHEEL_SPEED_RL",
            "WHEEL_SPEED_RR",
        ]
        .iter()
        .enumerate()
        {
            wheels[index] = f64::from(float(
                self.signal("WHEEL_SPEEDS", key, now)? * ((1. / 3.6) * self.config.factor),
            )?);
        }
        let mut speeds = ret.reborrow().init_wheel_speeds();
        speeds.set_fl(float(wheels[0])?);
        speeds.set_fr(float(wheels[1])?);
        speeds.set_rl(float(wheels[2])?);
        speeds.set_rr(float(wheels[3])?);
        let wheel = (wheels[0] + wheels[1] + wheels[2] + wheels[3]) / 4.;
        let weight = interp(wheel, &[1., 6.], &[0., 1.])?;
        let raw = float(
            (1. - weight)
                * self.signal("ENGINE_DATA", "XMISSION_SPEED", now)?
                * (1. / 3.6)
                * self.config.factor
                + weight * wheel,
        )?;
        ret.set_v_ego_raw(raw);
        let [speed, accel] = self.speed.update(f64::from(raw));
        ret.set_v_ego(float(speed)?);
        ret.set_a_ego(float(accel)?);
        self.extras.dash_speed_seen = self.extras.dash_speed_seen
            || self.signal("CAR_SPEED", "ROUGH_CAR_SPEED_2", now)? > 1e-3;
        if self.extras.dash_speed_seen {
            ret.set_v_ego_cluster(float(
                self.signal("CAR_SPEED", "ROUGH_CAR_SPEED_2", now)?
                    * if self.is_metric {
                        1. / 3.6
                    } else {
                        1.609344 * (1. / 3.6)
                    },
            )?);
        }
        ret.set_steering_angle_deg(float(self.signal(
            "STEERING_SENSORS",
            "STEER_ANGLE",
            now,
        )?)?);
        ret.set_steering_rate_deg(float(self.signal(
            "STEERING_SENSORS",
            "STEER_ANGLE_RATE",
            now,
        )?)?);
        let left = self.signal("SCM_FEEDBACK", "LEFT_BLINKER", now)? != 0.;
        let right = self.signal("SCM_FEEDBACK", "RIGHT_BLINKER", now)? != 0.;
        if left {
            self.extras.right_blinker_cnt = 0;
            if !self.extras.left_blinker_prev {
                self.extras.left_blinker_cnt = 250;
            }
        }
        if right {
            self.extras.left_blinker_cnt = 0;
            if !self.extras.right_blinker_prev {
                self.extras.right_blinker_cnt = 250;
            }
        }
        self.extras.left_blinker_cnt = self.extras.left_blinker_cnt.saturating_sub(1);
        self.extras.right_blinker_cnt = self.extras.right_blinker_cnt.saturating_sub(1);
        self.extras.left_blinker_prev = left;
        self.extras.right_blinker_prev = right;
        ret.set_left_blinker(left || self.extras.left_blinker_cnt > 0);
        ret.set_right_blinker(right || self.extras.right_blinker_cnt > 0);
        ret.set_brake_hold_active(self.signal("VSA_STATUS", "BRAKE_HOLD_ACTIVE", now)? == 1.);
        if self.config.bosch()
            || matches!(
                self.config.candidate.as_str(),
                "HONDA_CIVIC" | "HONDA_ODYSSEY" | "HONDA_ODYSSEY_CHN"
            )
        {
            ret.set_parking_brake(self.signal("EPB_STATUS", "EPB_STATE", now)? != 0.);
        }
        match self.config.transmission {
            TransmissionType::Manual => {
                let value = self.signal("GEARBOX_ALT_2", "GEAR_MT", now)?;
                ret.set_clutch_pressed(value == 0.);
                ret.set_gear_shifter(if value == 14. {
                    GearShifter::Reverse
                } else {
                    GearShifter::Drive
                });
            }
            TransmissionType::Unknown
            | TransmissionType::Automatic
            | TransmissionType::Cvt
            | TransmissionType::Direct => {
                let gear = self
                    .signal(self.gearbox, "GEAR_SHIFTER", now)?
                    .to_i64()
                    .ok_or(Error::Numeric)?;
                let address = self.pt.dbc.message(self.gearbox)?.address;
                ret.set_gear_shifter(state_helpers::parse_gear(
                    self.defs
                        .get(&address)
                        .and_then(|d| d.get("GEAR_SHIFTER"))
                        .and_then(|d| d.get(&gear))
                        .map(String::as_str),
                ));
            }
        }
        let gas = float(self.signal("POWERTRAIN_DATA", "PEDAL_GAS", now)?)?;
        ret.set_gas(gas);
        ret.set_gas_pressed(gas > 1e-5);
        let torque = float(self.signal("STEER_STATUS", "STEER_TORQUE_SENSOR", now)?)?;
        ret.set_steering_torque(torque);
        ret.set_steering_torque_eps(float(self.signal(
            "STEER_MOTOR_TORQUE",
            "MOTOR_TORQUE",
            now,
        )?)?);
        let threshold = if matches!(self.config.candidate.as_str(), "ACURA_RDX" | "HONDA_CRV_EU") {
            400.
        } else {
            1200.
        };
        ret.set_steering_pressed(torque.abs() > threshold);
        Ok(())
    }
}
