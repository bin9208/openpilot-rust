use super::{limits, Error};
use openpilot_cereal::car_capnp::car_control::actuators::LongControlState;
use openpilot_control_policy::math::clip;
use std::collections::VecDeque;

#[derive(Debug)]
pub struct HyundaiJerk {
    pub jerk: f64,
    pub jerk_u: f64,
    pub jerk_l: f64,
    pub band_upper: f64,
    pub band_lower: f64,
    pub carrot_cruise: u8,
    pub carrot_accel: f64,
    history: VecDeque<f64>,
    error_values: [f64; 40],
}

impl Default for HyundaiJerk {
    fn default() -> Self {
        Self {
            jerk: 0.,
            jerk_u: 0.,
            jerk_l: 0.,
            band_upper: 0.,
            band_lower: 0.,
            carrot_cruise: 1,
            carrot_accel: 0.,
            history: VecDeque::with_capacity(50),
            error_values: [0.; 40],
        }
    }
}

pub struct JerkInput {
    pub canfd: bool,
    pub state: LongControlState,
    pub accel: f64,
    pub measured_accel: f64,
    pub actuator_jerk: f64,
    pub brake: bool,
    pub gas: bool,
}

pub struct CruiseInput {
    pub decel: f64,
    pub atc_decel: f64,
    pub atc_distance: f64,
    pub carrot_cruise: i32,
    pub override_active: bool,
    pub soft_hold: bool,
    pub stopping: bool,
    pub speed: f64,
    pub target_accel: f64,
    pub accel: f64,
    pub measured_accel: f64,
}

impl HyundaiJerk {
    pub fn make(&mut self, i: JerkInput) -> Result<(), Error> {
        let min = if i.canfd { 1. } else { 0.5 };
        self.jerk = match i.state {
            LongControlState::Stopping => min / 2. - i.measured_accel,
            LongControlState::Pid => i.actuator_jerk,
            LongControlState::Off | LongControlState::Starting => 0.,
        };
        if i.state == LongControlState::Off {
            self.jerk_u = 5.;
            self.jerk_l = 5.;
            self.band_upper = 0.;
            self.band_lower = 0.;
            self.history.clear();
            self.error_values = [0.; 40];
        } else if i.canfd {
            let active = i.state == LongControlState::Pid && !i.brake && !i.gas;
            let mut filtered = 0.;
            if active {
                if self.history.len() == 50 {
                    self.history.pop_front();
                }
                self.history.push_back(i.accel);
                let mut sustained = false;
                let mut error = 0.;
                if self.history.len() == 50 {
                    if let Some(delayed) = self.history.front() {
                        sustained =
                            *delayed < -1. && i.accel < -1. && (i.accel - delayed).abs() < 0.5;
                        if sustained {
                            error = (i.measured_accel - delayed).min(i.measured_accel - i.accel);
                        }
                    }
                }
                if sustained && self.jerk <= 0.1 && error > 0. {
                    self.error_values.rotate_left(1);
                    self.error_values[39] = error;
                    filtered = source_float_sum(&self.error_values) / 40.;
                } else {
                    self.error_values = [0.; 40];
                }
            } else {
                self.history.clear();
                self.error_values = [0.; 40];
            }
            [self.jerk_u, self.jerk_l] = limits::jerk_limits(i.accel, self.jerk, filtered)?;
            self.band_upper = 0.;
            self.band_lower = 0.;
        } else {
            let up = self.jerk * 2.;
            let down = -self.jerk * 4.;
            self.jerk_u = if up > min { up.min(5.) } else { min };
            self.jerk_l = if down > 1. { down.min(5.) } else { 1. };
            self.band_upper = clip(0.9 + i.accel * 0.2, 0., 1.2);
            self.band_lower = clip(0.8 + i.accel * 0.2, 0., 1.2);
        }
        Ok(())
    }

    pub fn check_cruise(&mut self, i: CruiseInput) {
        let decel = if i.atc_decel >= 0. && i.atc_distance > 0. && i.atc_distance < 500. {
            i.decel.max(i.atc_decel)
        } else {
            i.decel
        };
        self.carrot_cruise = 0;
        if i.carrot_cruise > 0
            && !i.override_active
            && !i.soft_hold
            && !i.stopping
            && i.speed > 10. / 3.6
        {
            if decel < 0. {
                if i.target_accel > -0.1 || i.accel > -0.1 {
                    self.carrot_cruise = 1;
                    self.carrot_accel = 0.;
                }
            } else {
                self.carrot_cruise = 2;
                self.carrot_accel = i.accel.min(-decel * 0.01).max(self.carrot_accel - 0.01);
            }
        }
        if self.carrot_cruise == 0 {
            self.carrot_accel = i.measured_accel;
        }
    }
}

pub fn source_float_sum(values: &[f64]) -> f64 {
    let mut total = 0f64;
    let mut correction = 0f64;
    for value in values {
        let next = total + value;
        correction += if total.abs() >= value.abs() {
            (total - next) + value
        } else {
            (value - next) + total
        };
        total = next;
    }
    if correction != 0. && correction.is_finite() {
        total + correction
    } else {
        total
    }
}
