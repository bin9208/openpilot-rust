use super::{controller::Controller, state::State, Error};
use num_traits::ToPrimitive;
use openpilot_cereal::car_capnp::{
    car_control::{self, h_u_d_control::VisualAlert},
    car_state,
};
use openpilot_control_policy::math::{clip, interp};

pub(super) struct Prepared {
    pub accel: f64,
    pub gas_brake: [f64; 2],
    pub torque: i64,
    pub fcw: u8,
    pub steer: u8,
}
impl Controller {
    pub(super) fn prepare(
        &mut self,
        state: &State,
        cc: car_control::Reader<'_>,
    ) -> Result<Prepared, Error> {
        let act = cc.get_actuators()?;
        let speed = f64::from(
            state
                .out
                .get_root_as_reader::<car_state::Reader>()?
                .get_v_ego(),
        );
        let accel = if cc.get_long_active() {
            f64::from(act.get_accel())
        } else {
            0.
        };
        let [gas, brake] = if cc.get_long_active() && !self.config.bosch() {
            let creep = if speed < 2.3 {
                (2.3 - speed) / 2.3 * 0.15
            } else {
                0.
            };
            let gb = accel / 4.8 - creep;
            [clip(gb, 0., 1.), clip(-gb, 0., 1.)]
        } else {
            [0., 0.]
        };
        self.history.last_torque = clip(
            f64::from(act.get_torque()),
            self.history.last_torque - f64::from(self.limits.steer_delta_down) * 0.01,
            self.history.last_torque + f64::from(self.limits.steer_delta_up) * 0.01,
        );
        let hysteresis_brake = if (brake < 0.02 && !self.history.braking) || brake < 0.005 {
            0.
        } else {
            brake
        };
        self.history.braking = hysteresis_brake > 0.;
        if hysteresis_brake == 0. {
            self.history.brake_steady = 0.;
        } else if hysteresis_brake > self.history.brake_steady + 0.01 {
            self.history.brake_steady = hysteresis_brake - 0.01;
        } else if hysteresis_brake < self.history.brake_steady - 0.01 {
            self.history.brake_steady = hysteresis_brake + 0.01;
        }
        self.history.brake_last = clip(
            self.history.brake_steady,
            self.history.brake_last - 2.,
            self.history.brake_last + 0.01,
        );
        let (fcw, steer) = match cc.get_hud_control()?.get_visual_alert()? {
            VisualAlert::Fcw => (1, 0),
            VisualAlert::SteerRequired | VisualAlert::Ldw => (0, 1),
            VisualAlert::None
            | VisualAlert::BrakePressed
            | VisualAlert::WrongGear
            | VisualAlert::SeatbeltUnbuckled
            | VisualAlert::SpeedTooHigh => (0, 0),
        };
        let torque = interp(
            -self.history.last_torque * self.limits.steer_max,
            &self.limits.steer_lookup_bp,
            &self.limits.steer_lookup_v,
        )?
        .to_i64()
        .ok_or(Error::Numeric)?;
        Ok(Prepared {
            accel,
            gas_brake: [gas, brake],
            torque,
            fcw,
            steer,
        })
    }
}
