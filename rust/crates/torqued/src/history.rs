use crate::{
    interpolation::{engagement_times, interp},
    Error,
};
use openpilot_calibrationd::orientation::rotation;
use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub enum Input {
    Control {
        time: f64,
        active: bool,
    },
    Output {
        time: f64,
        torque: f64,
    },
    State {
        time: f64,
        speed: f64,
        pressed: bool,
    },
    Calibration(Vec<f64>),
    Delay(f64),
    Pose {
        time: f64,
        roll: f64,
        angular: [f64; 3],
        valid: bool,
    },
}
#[derive(Default)]
struct Samples(VecDeque<[f64; 3]>);
const HISTORY_CAPACITY: usize = 100;
impl Samples {
    fn push(&mut self, row: [f64; 3]) {
        if self.0.len() == HISTORY_CAPACITY {
            self.0.pop_front();
        }
        self.0.push_back(row);
    }
    fn at(&self, query: &[f64], column: usize) -> Result<Vec<f64>, Error> {
        // Preserve the deque's logical order and the interpolation arithmetic;
        // the bounded columns need no heap allocation for each lookup.
        let mut times = [0.; HISTORY_CAPACITY];
        let mut values = [0.; HISTORY_CAPACITY];
        for (index, row) in self.0.iter().enumerate() {
            times[index] = row[0];
            values[index] = row[column];
        }
        interp(query, &times[..self.0.len()], &values[..self.0.len()])
    }
}
pub struct History {
    control: Samples,
    output: Samples,
    state: Samples,
    pub lag: f64,
    device_from_calib: [[f64; 3]; 3],
}
impl Default for History {
    fn default() -> Self {
        Self {
            control: Samples::default(),
            output: Samples::default(),
            state: Samples::default(),
            lag: 0.,
            device_from_calib: rotation([0.; 3]),
        }
    }
}
impl History {
    pub fn clear_samples(&mut self) {
        self.control = Samples::default();
        self.output = Samples::default();
        self.state = Samples::default();
    }
    pub fn handle(&mut self, input: Input) -> Result<Option<[f64; 2]>, Error> {
        match input {
            Input::Control { time, active } => {
                self.control.push([time + self.lag, f64::from(active), 0.])
            }
            Input::Output { time, torque } => self.output.push([time + self.lag, -torque, 0.]),
            Input::State {
                time,
                speed,
                pressed,
            } => self
                .state
                .push([time + self.lag, speed, f64::from(pressed)]),
            Input::Delay(lag) => self.lag = lag,
            Input::Calibration(rpy) => {
                self.device_from_calib = rotation(
                    rpy.try_into()
                        .map_err(|_| Error::Contract("calibration RPY must have three entries"))?,
                );
            }
            Input::Pose {
                time,
                roll,
                angular,
                valid,
            } => {
                if self.output.0.len() == HISTORY_CAPACITY && valid {
                    let times = engagement_times(time, self.lag)?;
                    let active = self.control.at(&times, 1)?;
                    let pressed = self.state.at(&times, 2)?;
                    let speed = self.state.at(&[time], 1)?[0];
                    let steer = self.output.at(&[time], 1)?[0];
                    let yaw = self.device_from_calib[0][2] * angular[0]
                        + self.device_from_calib[1][2] * angular[1]
                        + self.device_from_calib[2][2] * angular[2];
                    let acceleration = speed * yaw - roll.sin() * 9.81;
                    if active.iter().all(|v| *v != 0.)
                        && pressed.iter().all(|v| *v == 0.)
                        && speed > 15.
                        && steer.abs() > 0.02
                    {
                        return Ok(Some([steer, acceleration]));
                    }
                }
            }
        }
        Ok(None)
    }
}
