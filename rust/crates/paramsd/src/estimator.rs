use crate::{
    kalman::CarKalman,
    model,
    types::{clip, diagonal, hysteresis, Car, Event, Input, Parameters},
    Error,
};
use openpilot_locationd::orientation::{self, Matrix};

pub struct Estimator {
    pub kf: CarKalman,
    initial: [f64; 9],
    covariance: Vec<f64>,
    min_ratio: f64,
    max_ratio: f64,
    calibration: Matrix,
    pub observed: [f64; 3],
    pub active: bool,
    average: f64,
    offset: f64,
    roll: f64,
    average_valid: bool,
    offset_valid: bool,
    roll_valid: bool,
    pub logs: Vec<(String, String)>,
}
impl Estimator {
    pub fn new(
        car: &Car,
        ratio: f64,
        stiffness: f64,
        offset: f64,
        covariance: Option<Vec<f64>>,
    ) -> Result<Self, Error> {
        let mut initial = model::INITIAL_X;
        initial[0] = stiffness;
        initial[1] = ratio;
        initial[2] = offset;
        let mut out = Self {
            kf: CarKalman::new(&car.globals)?,
            initial,
            covariance: covariance.unwrap_or_else(|| diagonal(&model::INITIAL_P)),
            min_ratio: 0.5 * car.ratio,
            max_ratio: 2. * car.ratio,
            calibration: orientation::IDENTITY,
            observed: [0.; 3],
            active: false,
            average: 0.,
            offset: 0.,
            roll: 0.,
            average_valid: true,
            offset_valid: true,
            roll_valid: true,
            logs: Vec::new(),
        };
        out.reset(None)?;
        Ok(out)
    }
    pub fn reset(&mut self, time: Option<f64>) -> Result<(), Error> {
        self.kf.reset(time, &self.initial, &self.covariance)?;
        self.offset = self.initial[2].to_degrees();
        self.average = self.offset;
        self.roll = 0.;
        self.active = false;
        Ok(())
    }
    pub fn handle(&mut self, event: Event) -> Result<(), Error> {
        let mut time = event.time;
        match event.input {
            Input::Pose(pose) => {
                time = pose.time;
                let mut yaw = orientation::apply(self.calibration, pose.angular)[2];
                let mut yaw_std = orientation::rotate_std(self.calibration, pose.angular_std)[2];
                if !(pose.yaw_valid && yaw_std > 0. && yaw_std < 10. && yaw.abs() < 1.) {
                    yaw = 0.;
                    yaw_std = 10_f64.to_radians();
                }
                self.observed[1] = yaw;
                let std = if pose.roll_std.is_nan() {
                    1_f64.to_radians()
                } else {
                    pose.roll_std
                };
                let (roll, std) = if std < 1.5_f64.to_radians()
                    && pose.roll > (-10_f64).to_radians()
                    && pose.roll < 10_f64.to_radians()
                    && pose.sensors_ok
                {
                    (pose.roll, 2. * std)
                } else {
                    (0., 10_f64.to_radians())
                };
                self.observed[2] = clip(
                    roll,
                    self.observed[2] - 1_f64.to_radians(),
                    self.observed[2] + 1_f64.to_radians(),
                );
                if self.active {
                    if pose.posenet_ok {
                        self.kf.observe(time, 25, -yaw, Some(yaw_std.powi(2)))?;
                        self.kf
                            .observe(time, 31, self.observed[2], Some(std.powi(2)))?;
                    }
                    self.kf.observe(time, 27, 0., None)?;
                    let x = self.kf.snapshot()?.x;
                    self.kf.observe(time, 28, x[0], None)?;
                    self.kf.observe(time, 29, x[1], None)?;
                }
            }
            Input::Calibration(rpy) => {
                let matrix = orientation::rotation(rpy);
                self.calibration = std::array::from_fn(|i| std::array::from_fn(|j| matrix[j][i]));
            }
            Input::Car { speed, steering } => {
                self.observed[0] = speed;
                self.active = speed > 1. && steering.abs() < 45.;
                if self.active {
                    self.kf.observe(time, 26, steering.to_radians(), None)?;
                    self.kf.observe(time, 30, speed, None)?;
                }
            }
            Input::Gps { .. } => {}
        }
        if !self.active {
            self.kf.pause(time);
        }
        Ok(())
    }
    pub fn parameters(&mut self, valid: bool, debug: bool) -> Result<Parameters, Error> {
        let state = self.kf.snapshot()?;
        let std: [f64; 9] = std::array::from_fn(|i| state.covariance[i * 9 + i].sqrt());
        let mut x = state.x;
        if x.iter().any(|value| !value.is_finite()) {
            self.logs.push((
                "error".into(),
                "NaN in liveParameters estimate. Resetting to default values".into(),
            ));
            self.reset(Some(state.time))?;
            x = self.kf.snapshot()?.x;
        }
        self.average = clip(x[2].to_degrees(), self.average - 1., self.average + 1.);
        self.offset = clip(
            (x[2] + x[3]).to_degrees(),
            self.offset - 1.,
            self.offset + 1.,
        );
        self.roll = clip(
            x[8],
            self.roll - 1_f64.to_radians(),
            self.roll + 1_f64.to_radians(),
        );
        let sensor_valid = !(self.active && self.observed[0] > 10.)
            || (self.observed[0] * (x[6] + self.observed[1])).abs() < 4.;
        self.average_valid = hysteresis(self.average_valid, self.average, 10., 8.);
        self.offset_valid = hysteresis(self.offset_valid, self.offset, 10., 8.);
        self.roll_valid = hysteresis(
            self.roll_valid,
            self.roll,
            10_f64.to_radians(),
            8_f64.to_radians(),
        );
        let ratio = f64::from(x[1] as f32);
        let stiffness = f64::from(x[0] as f32);
        let ratio_valid = self.min_ratio <= ratio && ratio <= self.max_ratio;
        let stiffness_valid = (0.2..=5.).contains(&stiffness);
        Ok(Parameters {
            valid,
            sensor_valid,
            ratio_valid,
            stiffness_valid,
            average_valid: self.average_valid,
            offset_valid: self.offset_valid,
            estimate_valid: self.average_valid
                && self.offset_valid
                && self.roll_valid
                && std[8] < 1.5_f64.to_radians()
                && stiffness_valid
                && ratio_valid,
            x: x.try_into().map_err(|_| Error::Contract("state size"))?,
            std,
            average: self.average,
            offset: self.offset,
            roll: self.roll,
            debug,
        })
    }
}
