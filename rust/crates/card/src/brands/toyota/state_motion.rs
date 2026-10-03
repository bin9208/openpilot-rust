use super::{state::State, Error, DISABLE_RADAR, SECOC};
use crate::state_helpers;
use num_traits::ToPrimitive;
use openpilot_cereal::car_capnp::car_state;

pub(super) fn float(value: f64) -> Result<f32, Error> {
    value.to_f32().ok_or(Error::Numeric)
}
impl State {
    pub(super) fn body(
        &mut self,
        ret: &mut car_state::Builder<'_>,
        now: u64,
    ) -> Result<i64, Error> {
        if self.config.flags & SECOC == 0 {
            self.extras.gvc = self.signal(false, "VSC1S07", "GVC", now)?;
        }
        let mut door = false;
        for key in [
            "DOOR_OPEN_FL",
            "DOOR_OPEN_FR",
            "DOOR_OPEN_RL",
            "DOOR_OPEN_RR",
        ] {
            door |= self.signal(false, "BODY_CONTROL_STATE", key, now)? != 0.;
        }
        ret.set_door_open(door);
        ret.set_seatbelt_unlatched(
            self.signal(
                false,
                "BODY_CONTROL_STATE",
                "SEATBELT_DRIVER_UNLATCHED",
                now,
            )? != 0.,
        );
        ret.set_parking_brake(
            self.signal(false, "BODY_CONTROL_STATE", "PARKING_BRAKE", now)? == 1.,
        );
        ret.set_brake_pressed(self.signal(false, "BRAKE_MODULE", "BRAKE_PRESSED", now)? != 0.);
        ret.set_brake_hold_active(
            self.signal(false, "ESP_CONTROL", "BRAKE_HOLD_ACTIVE", now)? == 1.,
        );
        let gear;
        if self.config.flags & SECOC != 0 {
            self.extras.secoc_synchronization =
                Some(self.copied(false, "SECOC_SYNCHRONIZATION", now)?);
            let gas = self.signal(false, "GAS_PEDAL", "GAS_PEDAL_USER", now)?;
            ret.set_gas(float(gas)?);
            ret.set_gas_pressed(gas > 0.);
            gear = self.signal(false, "GEAR_PACKET_HYBRID", "GEAR", now)?;
        } else {
            ret.set_gas_pressed(self.signal(false, "PCM_CRUISE", "GAS_RELEASED", now)? == 0.);
            gear = self.signal(false, "GEAR_PACKET", "GEAR", now)?;
            if !self.config.dsu && self.config.flags & DISABLE_RADAR == 0 {
                ret.set_stock_aeb(
                    self.signal(
                        self.acc_camera(),
                        "PRE_COLLISION",
                        "PRECOLLISION_ACTIVE",
                        now,
                    )? != 0.
                        && self.signal(self.acc_camera(), "PRE_COLLISION", "FORCE", now)? < -1e-5,
                );
            }
            if self.config.candidate != "TOYOTA_MIRAI" {
                ret.set_engine_rpm(float(self.signal(false, "ENGINE_RPM", "RPM", now)?)?);
            }
        }
        gear.to_i64().ok_or(Error::Numeric)
    }
    pub(super) fn motion(
        &mut self,
        ret: &mut car_state::Builder<'_>,
        gear: i64,
        now: u64,
    ) -> Result<(), Error> {
        let mut wheels = [0.; 4];
        for (target, key) in wheels.iter_mut().zip([
            "WHEEL_SPEED_FL",
            "WHEEL_SPEED_FR",
            "WHEEL_SPEED_RL",
            "WHEEL_SPEED_RR",
        ]) {
            *target = self.signal(false, "WHEEL_SPEEDS", key, now)?;
        }
        let wheels = state_helpers::wheel_speeds(wheels, self.config.wheel_factor, 1. / 3.6);
        let [fl, fr, rl, rr] = [
            float(wheels[0])?,
            float(wheels[1])?,
            float(wheels[2])?,
            float(wheels[3])?,
        ];
        let mut ws = ret.reborrow().init_wheel_speeds();
        ws.set_fl(fl);
        ws.set_fr(fr);
        ws.set_rl(rl);
        ws.set_rr(rr);
        let raw = float((f64::from(fl) + f64::from(fr) + f64::from(rl) + f64::from(rr)) / 4.)?;
        ret.set_v_ego_raw(raw);
        let [speed, accel] = self.speed.update(f64::from(raw));
        let speed = float(speed)?;
        ret.set_v_ego(speed);
        ret.set_a_ego(float(accel)?);
        ret.set_v_ego_cluster(float(f64::from(speed) * 1.015)?);
        ret.set_standstill(f64::from(raw.abs()) < 1e-3);
        let angle = float(
            self.signal(false, "STEER_ANGLE_SENSOR", "STEER_ANGLE", now)?
                + self.signal(false, "STEER_ANGLE_SENSOR", "STEER_FRACTION", now)?,
        )?;
        let rate = float(self.signal(false, "STEER_ANGLE_SENSOR", "STEER_RATE", now)?)?;
        ret.set_steering_angle_deg(angle);
        ret.set_steering_rate_deg(rate);
        let torque_angle = self.signal(false, "STEER_TORQUE_SENSOR", "STEER_ANGLE", now)?;
        if torque_angle.abs() > 1e-3
            && self.signal(
                false,
                "STEER_TORQUE_SENSOR",
                "STEER_ANGLE_INITIALIZING",
                now,
            )? == 0.
        {
            self.extras.accurate_steer_angle_seen = true;
        }
        if self.extras.accurate_steer_angle_seen {
            if angle.abs() < 90. && rate.abs() < 100. && self.pt.can_valid() {
                let offset = torque_angle - f64::from(angle);
                let alpha = 0.01 / (60. + 0.01);
                self.extras.angle_offset = Some(
                    self.extras
                        .angle_offset
                        .map_or(offset, |previous| (1. - alpha) * previous + alpha * offset),
                );
            }
            if let Some(offset) = self.extras.angle_offset {
                ret.set_steering_angle_offset_deg(float(offset)?);
                ret.set_steering_angle_deg(float(torque_angle - offset)?);
            }
        }
        let gear_message = if self.config.flags & SECOC != 0 {
            "GEAR_PACKET_HYBRID"
        } else {
            "GEAR_PACKET"
        };
        let address = self.pt.dbc.message(gear_message)?.address;
        ret.set_gear_shifter(state_helpers::parse_gear(
            self.defs
                .get(&address)
                .and_then(|d| d.get("GEAR"))
                .and_then(|d| d.get(&gear))
                .map(String::as_str),
        ));
        let turn = self.signal(false, "BLINKERS_STATE", "TURN_SIGNALS", now)?;
        ret.set_left_blinker(turn == 1.);
        ret.set_right_blinker(turn == 2.);
        let torque =
            float(self.signal(false, "STEER_TORQUE_SENSOR", "STEER_TORQUE_DRIVER", now)?)?;
        ret.set_steering_torque(torque);
        ret.set_steering_torque_eps(float(
            self.signal(false, "STEER_TORQUE_SENSOR", "STEER_TORQUE_EPS", now)?
                * self.config.eps_scale,
        )?);
        ret.set_steering_pressed(torque.abs() > 100.);
        let lka = self.signal(false, "EPS_STATUS", "LKA_STATE", now)?;
        let mut temp = [0., 9., 11., 21., 25.].contains(&lka);
        let mut permanent = [3., 17.].contains(&lka);
        if self.config.angle {
            if !temp {
                temp = [0., 9., 11., 21., 25.].contains(&self.signal(
                    false,
                    "EPS_STATUS",
                    "LTA_STATE",
                    now,
                )?);
            }
            if !permanent {
                permanent =
                    [3., 17.].contains(&self.signal(false, "EPS_STATUS", "LTA_STATE", now)?);
            }
            ret.set_vehicle_sensors_invalid(!self.extras.accurate_steer_angle_seen);
        }
        ret.set_steer_fault_temporary(temp);
        ret.set_steer_fault_permanent(permanent);
        Ok(())
    }
}
