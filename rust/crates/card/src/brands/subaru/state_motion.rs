use super::{
    state::{Bus, State},
    Error, HYBRID, PREGLOBAL,
};
use crate::state_helpers;
use num_traits::ToPrimitive;
use openpilot_cereal::car_capnp::car_state;

pub(super) fn float(value: f64) -> Result<f32, Error> {
    value.to_f32().ok_or(Error::Numeric)
}
impl State {
    pub(super) fn motion(
        &mut self,
        ret: &mut car_state::Builder<'_>,
        now: u64,
    ) -> Result<(), Error> {
        let (bus, name) = if self.flags & HYBRID != 0 {
            (Bus::Alt, "Throttle_Hybrid")
        } else {
            (Bus::Pt, "Throttle")
        };
        let gas = float(self.signal(bus, name, "Throttle_Pedal", now)? / 255.)?;
        ret.set_gas(gas);
        ret.set_gas_pressed(f64::from(gas) > 1e-5);
        let brake = if self.flags & PREGLOBAL != 0 {
            self.signal(Bus::Pt, "Brake_Pedal", "Brake_Pedal", now)? > 0.
        } else {
            self.signal(self.chassis_bus(), "Brake_Status", "Brake", now)? == 1.
        };
        ret.set_brake_pressed(brake);
        if self.flags & HYBRID == 0 {
            let fault = self.signal(self.distance_bus(), "ES_Distance", "Cruise_Fault", now)? != 0.;
            if self.longitudinal {
                ret.set_car_faulted_non_critical(fault);
            } else {
                ret.set_acc_faulted(fault);
            }
        }
        let mut wheels = [0.; 4];
        for (target, name) in wheels.iter_mut().zip(["FL", "FR", "RL", "RR"]) {
            *target = self.signal(self.chassis_bus(), "Wheel_Speeds", name, now)?;
        }
        let wheels = state_helpers::wheel_speeds(wheels, self.factor, 1. / 3.6);
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
        ret.set_v_ego(float(speed)?);
        ret.set_a_ego(float(accel)?);
        ret.set_standstill(raw == 0.);
        let left = self.signal(Bus::Pt, "Dashlights", "LEFT_BLINKER", now)? != 0.;
        let right = self.signal(Bus::Pt, "Dashlights", "RIGHT_BLINKER", now)? != 0.;
        self.extras.left_blinker_cnt = if left {
            50
        } else {
            self.extras.left_blinker_cnt.saturating_sub(1)
        };
        self.extras.right_blinker_cnt = if right {
            50
        } else {
            self.extras.right_blinker_cnt.saturating_sub(1)
        };
        ret.set_left_blinker(self.extras.left_blinker_cnt > 0);
        ret.set_right_blinker(self.extras.right_blinker_cnt > 0);
        if self.bsm {
            ret.set_left_blindspot(
                self.signal(Bus::Pt, "BSD_RCTA", "L_ADJACENT", now)? == 1.
                    || self.signal(Bus::Pt, "BSD_RCTA", "L_APPROACHING", now)? == 1.,
            );
            ret.set_right_blindspot(
                self.signal(Bus::Pt, "BSD_RCTA", "R_ADJACENT", now)? == 1.
                    || self.signal(Bus::Pt, "BSD_RCTA", "R_APPROACHING", now)? == 1.,
            );
        }
        let bus = if self.flags & HYBRID != 0 {
            Bus::Alt
        } else {
            Bus::Pt
        };
        let gear = self
            .signal(bus, "Transmission", "Gear", now)?
            .to_i64()
            .ok_or(Error::Numeric)?;
        let address = self.pt.dbc.message("Transmission")?.address;
        ret.set_gear_shifter(state_helpers::parse_gear(
            self.defs
                .get(&address)
                .and_then(|d| d.get("Gear"))
                .and_then(|d| d.get(&gear))
                .map(String::as_str),
        ));
        let angle = float(self.signal(Bus::Pt, "Steering_Torque", "Steering_Angle", now)?)?;
        ret.set_steering_angle_deg(angle);
        if self.flags & PREGLOBAL == 0 {
            if self.signal(Bus::Pt, "Steering_Torque", "COUNTER", now)? != 0. {
                self.extras.angle_rate =
                    (f64::from(angle) - self.extras.angle_previous_value) * 50.;
            }
            self.extras.angle_previous_value = f64::from(angle);
            ret.set_steering_rate_deg(float(self.extras.angle_rate)?);
        }
        let torque = float(self.signal(Bus::Pt, "Steering_Torque", "Steer_Torque_Sensor", now)?)?;
        ret.set_steering_torque(torque);
        ret.set_steering_torque_eps(float(self.signal(
            Bus::Pt,
            "Steering_Torque",
            "Steer_Torque_Output",
            now,
        )?)?);
        ret.set_steering_pressed(
            torque.abs()
                > if self.flags & PREGLOBAL != 0 {
                    75.
                } else {
                    80.
                },
        );
        Ok(())
    }
}
