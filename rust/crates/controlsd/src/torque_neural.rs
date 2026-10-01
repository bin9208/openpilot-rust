use crate::{
    config::{Config, Torque},
    inputs::Inputs,
    interface::ControlInterface,
    parameters::Parameters,
    Error,
};
use openpilot_control_policy::{
    drive::{CONTROL_N, TIMES},
    math::{interp, maximum},
};
use std::collections::VecDeque;

pub struct Neural {
    pub jerk_time: f64,
    pub accel_friction: f64,
    pub future_times: [f64; 4],
    pub rolls: VecDeque<f64>,
    pub accelerations: VecDeque<f64>,
}
pub struct Acceleration {
    pub desired: f64,
    pub actual: f64,
    pub deadzone: f64,
    pub actual_jerk: f64,
    pub setpoint: f64,
    pub measurement: f64,
}
impl Neural {
    pub fn new(
        config: &Config,
        interface: &ControlInterface,
        params: &mut impl Parameters,
    ) -> Result<Self, Error> {
        let jerk_time = if interface.use_nnff || interface.use_nnff_lite {
            params.float("SteerActuatorDelay")? * 0.01 + 0.3
        } else {
            0.
        };
        let offset = config.steer_delay + 0.2;
        Ok(Self {
            jerk_time,
            accel_friction: 0.7,
            future_times: [0.3 + offset, 0.6 + offset, 1. + offset, 1.5 + offset],
            rolls: VecDeque::new(),
            accelerations: VecDeque::new(),
        })
    }
    pub fn response(
        &mut self,
        interface: &ControlInterface,
        tuning: &Torque,
        input: &Inputs,
        accel: Acceleration,
    ) -> Result<[f64; 2], Error> {
        let cs = &input.car;
        let model = &input.model;
        let good = model.orientation_x.len() >= CONTROL_N;
        let mut lookahead_jerk = 0.;
        let mut jerk_setpoint = 0.;
        let mut jerk_measurement = 0.;
        if good && (interface.use_nnff || interface.use_nnff_lite) {
            if model.acceleration_y.len() != TIMES.len() {
                return Err(Error::Contract("model acceleration dimensions"));
            }
            let lookahead = interp(cs.speed, &[9., 30.], &[1.4, 2.])?;
            let upper = TIMES
                .iter()
                .position(|time| *time > lookahead)
                .unwrap_or(16);
            let jerk = model
                .acceleration_y
                .windows(2)
                .zip(TIMES.windows(2))
                .map(|(a, t)| (a[1] - a[0]) / (t[1] - t[0]))
                .collect::<Vec<_>>();
            let desired = (interp(self.jerk_time, &TIMES, &model.acceleration_y)? - accel.desired)
                / self.jerk_time;
            lookahead_jerk = lookahead_value(&jerk[5..upper], desired);
            let mut actual_jerk = accel.actual_jerk;
            if tuning.steering_angle || lookahead_jerk == 0. {
                lookahead_jerk = 0.;
                actual_jerk = 0.;
                self.accel_friction = 1.;
            }
            jerk_setpoint = 0.4 * lookahead_jerk;
            jerk_measurement = 0.4 * actual_jerk;
        }
        if interface.use_nnff && good {
            self.flux_response(
                interface,
                tuning,
                input,
                &accel,
                [lookahead_jerk, jerk_setpoint, jerk_measurement],
            )
        } else {
            let roll = input.live.roll * 9.81;
            let setpoint = interface.torque(
                [accel.setpoint, roll, cs.speed, cs.accel],
                tuning,
                jerk_setpoint,
                accel.deadzone,
                interface.use_nnff_lite,
                false,
            )?;
            let measurement = interface.torque(
                [accel.measurement, roll, cs.speed, cs.accel],
                tuning,
                jerk_measurement,
                accel.deadzone,
                interface.use_nnff_lite,
                false,
            )?;
            let error = accel.desired - accel.actual;
            let friction = if interface.use_nnff_lite {
                self.accel_friction * error + 0.4 * lookahead_jerk
            } else {
                error
            };
            let ff = interface.torque(
                [accel.desired - roll, roll, cs.speed, cs.accel],
                tuning,
                friction,
                accel.deadzone,
                true,
                true,
            )?;
            Ok([f64::from((setpoint - measurement) as f32), ff])
        }
    }
    fn flux_response(
        &mut self,
        interface: &ControlInterface,
        tuning: &Torque,
        input: &Inputs,
        accel: &Acceleration,
        jerk: [f64; 3],
    ) -> Result<[f64; 2], Error> {
        let cs = &input.car;
        let model = &input.model;
        let roll = input.live.roll;
        // controlsd fills CarControl pose fields only after lateral control, so
        // the source controller receives an empty orientation and pitch stays zero.
        for (deque, value) in [
            (&mut self.rolls, roll),
            (&mut self.accelerations, accel.desired),
        ] {
            if deque.len() == 30 {
                deque.pop_front();
            }
            deque.push_back(value);
        }
        let mut rolls: Vec<f64> = [0, 10, 20]
            .into_iter()
            .map(|i| self.rolls[i.min(self.rolls.len() - 1)])
            .collect();
        let mut accelerations: Vec<f64> = [0, 10, 20]
            .into_iter()
            .map(|i| self.accelerations[i.min(self.accelerations.len() - 1)])
            .collect();
        for time in self.future_times {
            let time = time + 0.5 * cs.accel * (time / maximum(cs.speed, 1.));
            rolls.push(
                (interp(time, &TIMES, &model.orientation_x)? + roll)
                    * interp(time, &TIMES, &model.orientation_y)?.cos(),
            );
            accelerations.push(interp(time, &TIMES, &model.acceleration_y)?);
        }
        let mut setpoint = vec![cs.speed, accel.setpoint, jerk[1], roll];
        setpoint.extend([accel.setpoint; 7]);
        setpoint.extend(&rolls);
        let mut measurement = vec![cs.speed, accel.measurement, jerk[2], roll];
        measurement.extend([accel.measurement; 7]);
        measurement.extend(&rolls);
        let mut error =
            f64::from((interface.neural(&setpoint)? - interface.neural(&measurement)?) as f32);
        let blend = interp(accel.desired.abs(), &[1., 2.], &[0., 1.])?;
        if blend > 0. {
            let torque = interface.neural(&[
                cs.speed,
                accel.setpoint - accel.measurement,
                jerk[1] - jerk[2],
                0.,
            ])?;
            if sign(error) == sign(torque) && error.abs() < torque.abs() {
                error = f64::from((error * (1. - blend) + torque * blend) as f32);
            }
        }
        let friction = self.accel_friction * (accel.setpoint - accel.measurement) + 0.4 * jerk[0];
        let mut feedforward = vec![cs.speed, accel.desired, friction, roll];
        feedforward.extend(accelerations);
        feedforward.extend(rolls);
        let ff = interface.neural(&feedforward)?;
        if interface
            .flux
            .as_ref()
            .is_some_and(|model| model.friction_override)
        {
            error = f64::from(
                (error
                    + interface.torque(
                        [0., 0., cs.speed, cs.accel],
                        tuning,
                        friction,
                        accel.deadzone,
                        true,
                        false,
                    )?) as f32,
            );
        }
        Ok([error, ff])
    }
}
fn sign(value: f64) -> i8 {
    if value > 0. {
        1
    } else if value < 0. {
        -1
    } else {
        0
    }
}
fn lookahead_value(future: &[f64], current: f64) -> f64 {
    if future.is_empty() {
        return current;
    }
    if future.iter().any(|value| sign(*value) != sign(current)) {
        return 0.;
    }
    let mut value = future[0];
    for &next in future.iter().skip(1).chain(std::iter::once(&current)) {
        if next.abs() < value.abs() {
            value = next;
        }
    }
    value
}
