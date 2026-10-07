use super::Error;
use openpilot_control_policy::{
    math::{clip, interp},
    vehicle::VehicleModel,
};
use serde::Serialize;

const MAX_LATERAL: f64 = 3. + 9.81 * 0.06;
pub fn limit(
    angle: f64,
    last: f64,
    raw: f64,
    current: f64,
    active: bool,
    vm: &VehicleModel,
) -> Result<f64, Error> {
    let speed = raw.max(1.);
    let mut delta = vm
        .steer(MAX_LATERAL / speed.powi(2), speed, 0.)?
        .to_degrees()
        * 0.02;
    let up = last * angle >= 0. && angle.abs() > last.abs();
    delta = delta
        .min(interp(
            speed,
            &[0., 5., 25.],
            if up { &[2.5, 1.5, 0.2] } else { &[5., 2., 0.3] },
        )?)
        .min(5.);
    let mut result = clip(angle, last - delta, last + delta);
    let max = vm
        .steer(MAX_LATERAL / speed.powi(2), speed, 0.)?
        .to_degrees();
    result = clip(result, -max, max);
    if !active {
        result = current;
    }
    Ok(clip(result, -360., 360.))
}
#[derive(Default, Serialize)]
pub struct Coop {
    pub apply_angle_last: f64,
    pub coop_apply_angle_sat_last: f64,
    pub angle_override: f64,
    pub resume_rate_limiter_delta: f64,
    pub resume_rate_limiter: f64,
}
pub struct SteeringInput {
    pub requested: f64,
    pub active: bool,
    pub speed: f64,
    pub raw_speed: f64,
    pub steering_angle: f64,
    pub steering_rate: f64,
    pub torque: f64,
}
impl Coop {
    pub fn update(&mut self, input: SteeringInput, vm: &VehicleModel) -> Result<f64, Error> {
        let SteeringInput {
            requested,
            active,
            speed,
            raw_speed: raw,
            steering_angle: steering,
            steering_rate: rate,
            torque,
        } = input;
        let phase = steering + rate / 8.;
        let mut angle = if !active {
            self.resume_rate_limiter_delta = 0.;
            self.resume_rate_limiter = phase;
            phase
        } else {
            let step = 300. * 0.02_f64.powi(2);
            self.resume_rate_limiter_delta = clip(
                5.,
                self.resume_rate_limiter_delta - step,
                self.resume_rate_limiter_delta + step,
            );
            self.resume_rate_limiter = clip(
                requested,
                self.resume_rate_limiter - self.resume_rate_limiter_delta,
                self.resume_rate_limiter + self.resume_rate_limiter_delta,
            );
            self.resume_rate_limiter
        };
        if !active {
            self.apply_angle_last = angle;
            self.angle_override = 0.;
            self.coop_apply_angle_sat_last = angle;
            return Ok(angle);
        }
        let apply_step = angle - self.apply_angle_last;
        self.apply_angle_last = angle;
        let gain = clip(
            vm.steer(2. / speed.max(1.).powi(2), speed, 0.)?
                .to_degrees(),
            -360.,
            360.,
        ) / 2.;
        let driver = torque - clip(torque, -0.5, 0.5);
        let neutral = if speed.abs() <= 0.1 {
            0.
        } else {
            self.angle_override / gain
        };
        let override_torque = driver - neutral;
        let target = driver * gain;
        let away = override_torque.abs() * 125. * 0.02;
        let center = override_torque.abs() * (5. / 0.02 / 2.) * 0.02;
        let down = if self.angle_override > 0. {
            center
        } else {
            away
        };
        let up = if self.angle_override < 0. {
            center
        } else {
            away
        };
        let mut slew = clip(target - self.angle_override, -down, up);
        let direction = slew * apply_step;
        if direction > 0. {
            slew -= clip(apply_step, -slew.abs(), slew.abs());
        } else if direction < 0. {
            slew -= override_torque.abs() / 2. * apply_step;
        }
        self.angle_override += slew;
        angle += self.angle_override;
        self.coop_apply_angle_sat_last = limit(
            angle,
            self.coop_apply_angle_sat_last,
            raw,
            steering,
            active,
            vm,
        )?;
        let error = angle - self.coop_apply_angle_sat_last;
        if self.angle_override * error > 0. {
            self.angle_override -=
                clip(error, -self.angle_override.abs(), self.angle_override.abs());
        }
        Ok(self.coop_apply_angle_sat_last)
    }
}
