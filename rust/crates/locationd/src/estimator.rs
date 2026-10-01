use crate::{
    bridge::ffi::Estimate,
    kalman::PoseKalman,
    orientation::{self, Matrix},
    types::{HandleResult, Input, Pose, Sensor, KINDS},
    Error,
};

pub struct Estimator {
    pub kf: PoseKalman,
    debug: bool,
    posenet_stds: [f64; 40],
    car_speed: f64,
    camodo_yawrate: [f64; 2],
    device_from_calib: Matrix,
    observations: [[f64; 3]; 4],
    errors: [[f64; 3]; 4],
    pub logs: Vec<(String, String)>,
}
impl Estimator {
    pub fn new(debug: bool) -> Result<Self, Error> {
        Ok(Self {
            kf: PoseKalman::new()?,
            debug,
            posenet_stds: [10.; 40],
            car_speed: 0.,
            camodo_yawrate: [0., 10.],
            device_from_calib: orientation::IDENTITY,
            observations: [[0.; 3]; 4],
            errors: [[0.; 3]; 4],
            logs: Vec::new(),
        })
    }
    fn log(&mut self, level: &str, text: &str) {
        self.logs.push((level.into(), text.into()));
    }
    fn valid_timestamp(&mut self, time: f64) -> Result<bool, Error> {
        let invalid = self.kf.snapshot()?.time - time > 0.8;
        if invalid {
            self.log(
                "warning",
                "Observation timestamp is older than the max rewind threshold of the filter",
            );
        }
        Ok(!invalid)
    }
    fn valid_sensor_time(&mut self, sensor: f64, logged: f64) -> bool {
        if sensor == 0. {
            return false;
        }
        if (sensor - logged).abs() > 0.1 {
            self.log(
                "warning",
                "Sensor reading ignored, sensor timestamp more than 100ms off from log time",
            );
            return false;
        }
        true
    }
    fn record(&mut self, index: usize, values: [f64; 3], result: &Estimate) -> Result<(), Error> {
        if result.observed {
            self.observations[index] = values;
            self.errors[index] = result
                .residual
                .as_slice()
                .try_into()
                .map_err(|_| Error::Contract("residual size"))?;
        }
        Ok(())
    }
    pub fn handle(&mut self, log_time: f64, input: Input) -> Result<HandleResult, Error> {
        let mut finite_check = None;
        let mut time = log_time;
        match input {
            Input::Sensor {
                kind,
                time: sensor_time,
                secondary,
                values,
            } => {
                if !self.valid_sensor_time(sensor_time, time)
                    || !self.valid_timestamp(sensor_time)?
                {
                    return Ok(HandleResult::TimingInvalid);
                }
                if secondary {
                    return Ok(HandleResult::SensorSourceInvalid);
                }
                let measurement = [-values[2], -values[1], -values[0]];
                let index = match kind {
                    Sensor::Acceleration => {
                        if orientation::norm(measurement) >= 100. {
                            return Ok(HandleResult::InputInvalid);
                        }
                        0
                    }
                    Sensor::Gyroscope => {
                        let bias = self.kf.snapshot()?.x[11];
                        let error = ((measurement[2] - bias) - self.camodo_yawrate[0]).abs();
                        let mut std = self.camodo_yawrate[1];
                        if self.car_speed < 5. && std < 0.02 {
                            std = 0.02;
                        }
                        let valid = error < 30. * std;
                        if orientation::norm(measurement) >= 10. || !valid {
                            return Ok(HandleResult::InputInvalid);
                        }
                        1
                    }
                };
                let result = self
                    .kf
                    .observe(sensor_time, KINDS[index], measurement, None)?;
                self.record(index, measurement, &result)?;
                finite_check = Some(result);
            }
            Input::Speed(speed) => self.car_speed = speed.abs(),
            Input::Calibration(Some(calibration)) => {
                if orientation::minimum(calibration) < -0.5
                    || orientation::maximum(calibration) > 0.5
                {
                    return Ok(HandleResult::InputInvalid);
                }
                self.device_from_calib = orientation::rotation(calibration);
            }
            Input::Camera {
                time: camera_time,
                rotation,
                translation,
                rotation_std,
                translation_std,
            } => {
                time = camera_time;
                if !self.valid_timestamp(time)? {
                    return Ok(HandleResult::TimingInvalid);
                }
                let rotation = orientation::apply(self.device_from_calib, rotation);
                let translation = orientation::apply(self.device_from_calib, translation);
                if orientation::norm(rotation) > 10. || orientation::norm(translation) > 200. {
                    return Ok(HandleResult::InputInvalid);
                }
                if orientation::minimum(rotation_std) <= 1e-5
                    || orientation::minimum(translation_std) <= 1e-5
                {
                    return Ok(HandleResult::InputInvalid);
                }
                if orientation::norm(rotation_std) > 100.
                    || orientation::norm(translation_std) > 2000.
                {
                    return Ok(HandleResult::InputInvalid);
                }
                self.posenet_stds.rotate_left(1);
                self.posenet_stds[39] = translation_std[0];
                let rotation_std =
                    orientation::rotate_std(self.device_from_calib, rotation_std.map(|v| v * 10.));
                let translation_std = orientation::rotate_std(
                    self.device_from_calib,
                    translation_std.map(|v| v * 4.),
                );
                let rot = self
                    .kf
                    .observe(time, 14, rotation, Some(rotation_std.map(|v| v * v)))?;
                let trans =
                    self.kf
                        .observe(time, 13, translation, Some(translation_std.map(|v| v * v)))?;
                self.camodo_yawrate = [rotation[2], rotation_std[2]];
                self.record(2, rotation, &rot)?;
                self.record(3, translation, &trans)?;
                finite_check = if trans.observed {
                    Some(trans)
                } else {
                    Some(rot)
                };
            }
            Input::Calibration(None) | Input::Ignored => (),
        }
        if let Some(result) = finite_check.filter(|result| result.observed) {
            if !result
                .x
                .iter()
                .chain(&result.covariance)
                .all(|value| value.is_finite())
            {
                self.log("error", "Non-finite values detected, kalman reset");
                self.kf.reset_default(Some(time))?;
            }
        }
        Ok(HandleResult::Success)
    }
    pub fn pose(
        &self,
        sensors_valid: bool,
        inputs_valid: bool,
        filter_valid: bool,
    ) -> Result<Pose, Error> {
        let snapshot = self.kf.snapshot()?;
        let x = snapshot
            .x
            .as_slice()
            .try_into()
            .map_err(|_| Error::Contract("state size"))?;
        let std = std::array::from_fn(|i| snapshot.covariance[i * 18 + i].sqrt());
        let old_mean = self.posenet_stds[..20].iter().sum::<f64>() / 20.;
        let new_mean = self.posenet_stds[20..].iter().sum::<f64>() / 20.;
        let spike = new_mean / old_mean > 4. && new_mean > 7.;
        let timestamp = crate::types::filter_timestamp(snapshot.time)?;
        Ok(Pose {
            x,
            std,
            observations: self.observations,
            errors: self.errors,
            debug: self.debug,
            timestamp,
            filter_valid,
            sensors_valid,
            inputs_valid,
            posenet_valid: !spike || self.car_speed <= 5.,
        })
    }
}
