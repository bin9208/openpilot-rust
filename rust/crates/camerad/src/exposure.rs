mod manual;
pub use manual::{ManualExposure, ParseError};

use serde::Serialize;
use thiserror::Error;

use crate::arithmetic::{multiply_add32, multiply_add64};

use crate::sensor::{Exposure, ExposureRegisters, ExposureScore, SensorError, SensorKind};

#[derive(Clone, Copy, Debug)]
pub enum CameraId {
    Wide,
    Road,
    Driver,
}

#[derive(Clone, Copy, Debug)]
pub struct FrameMeasurement {
    pub frame_id: u32,
    pub grey: f32,
    pub enabled: bool,
}

#[derive(Debug, Error)]
pub enum ExposureError {
    #[error("manual exposure input: {0}")]
    ManualInput(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error(transparent)]
    Sensor(#[from] SensorError),
}

#[derive(Clone, Debug, Serialize)]
pub struct ExposureState {
    #[serde(skip)]
    sensor: SensorKind,
    #[serde(skip)]
    camera: CameraId,
    pub exposure_time: i32,
    pub dc_gain_enabled: bool,
    pub dc_gain_weight: i32,
    pub gain_idx: i32,
    pub analog_gain_frac: f32,
    pub cur_ev: [f32; 3],
    pub best_ev_score: f32,
    pub new_exp_g: i32,
    pub new_exp_t: i32,
    pub measured_grey_fraction: f32,
    pub target_grey_fraction: f32,
}

impl ExposureState {
    pub fn new(sensor: SensorKind, camera: CameraId) -> Self {
        let config = sensor.config();
        let mut state = Self {
            sensor,
            camera,
            exposure_time: 5,
            dc_gain_enabled: false,
            dc_gain_weight: config.dc_gain_min_weight,
            gain_idx: config.analog_gain_rec_idx,
            analog_gain_frac: 0.0,
            cur_ev: [0.0; 3],
            best_ev_score: 0.0,
            new_exp_g: 0,
            new_exp_t: 0,
            measured_grey_fraction: 0.0,
            target_grey_fraction: 0.125,
        };
        let gain_index = match sensor {
            SensorKind::Ar0231 => 6,
            SensorKind::Ox03c10 | SensorKind::Os04c10 => 0,
        };
        let initial_ev = state.gain_factor() * config.sensor_analog_gains[gain_index] * 5.0;
        state.cur_ev.fill(initial_ev);
        state
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "source sensor conversion-gain weights are small integers"
    )]
    pub fn gain_factor(&self) -> f32 {
        let config = self.sensor.config();
        1.0 + self.dc_gain_weight as f32 * (config.dc_gain_factor - 1.0)
            / config.dc_gain_max_weight as f32
    }

    pub fn update(
        &mut self,
        frame: FrameMeasurement,
        manual: ManualExposure<'_>,
    ) -> Result<Option<ExposureRegisters>, ExposureError> {
        self.update_with_manual(frame, || Ok((manual.gain, manual.time)))
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "match the source's float/double rounding and bounded exposure integer conversion"
    )]
    pub fn update_with_manual<G: AsRef<str>, T: AsRef<str>>(
        &mut self,
        frame: FrameMeasurement,
        read_manual: impl FnOnce() -> Result<(G, T), ExposureError>,
    ) -> Result<Option<ExposureRegisters>, ExposureError> {
        if !frame.enabled {
            return Ok(None);
        }
        let config = self.sensor.config();
        let minimum = match self.camera {
            CameraId::Wide | CameraId::Road => 0.1,
            CameraId::Driver => 0.125,
        };
        let dt = 0.05_f32;
        let k_grey = (f64::from(dt / 10.0) / (1.0 + f64::from(dt / 10.0))) as f32;
        let k_ev = 0.5_f32;
        let previous_slot = usize::try_from(frame.frame_id.wrapping_sub(1) % 3)
            .map_err(|_| SensorError::GainIndex(self.gain_idx))?;
        let current_ev = self.cur_ev[previous_slot] * config.ev_scale;
        let new_target = (0.4
            - 0.3 * (1.0 + f64::from(config.target_grey_factor * current_ev)).log2()
                / 6000.0_f64.log2())
        .clamp(minimum, 0.4) as f32;
        let target = multiply_add64(
            1.0 - f64::from(k_grey),
            f64::from(self.target_grey_fraction),
            f64::from(k_grey * new_target),
        ) as f32;
        let mut desired = (current_ev / config.ev_scale * target / frame.grey)
            .clamp(config.min_ev, config.max_ev);
        let weight = ((1.0 - f64::from(k_ev)) / 3.0) as f32;
        desired = multiply_add32(
            k_ev,
            desired,
            multiply_add32(
                weight,
                self.cur_ev[2],
                multiply_add32(weight, self.cur_ev[0], weight * self.cur_ev[1]),
            ),
        );
        self.best_ev_score = 1e6;
        self.new_exp_g = 0;
        self.new_exp_t = 0;
        let mut enable_dc = self.dc_gain_enabled;
        if !enable_dc && target < config.dc_gain_on_grey {
            enable_dc = true;
            self.dc_gain_weight = config.dc_gain_min_weight;
        } else if enable_dc && target > config.dc_gain_off_grey {
            enable_dc = false;
            self.dc_gain_weight = config.dc_gain_max_weight;
        }
        if enable_dc && self.dc_gain_weight < config.dc_gain_max_weight {
            self.dc_gain_weight += 1;
        }
        if !enable_dc && self.dc_gain_weight > config.dc_gain_min_weight {
            self.dc_gain_weight -= 1;
        }
        let (gain, time) = read_manual()?;
        let manual = ManualExposure {
            gain: gain.as_ref(),
            time: time.as_ref(),
        };
        if !manual.gain.is_empty() && !manual.time.is_empty() {
            self.gain_idx = manual::stoi(manual.gain)?;
            self.exposure_time = manual::stoi(manual.time)?;
            self.new_exp_g = self.gain_idx;
            self.new_exp_t = self.exposure_time;
            enable_dc = false;
        } else {
            let min_gain = (self.gain_idx - 1).max(config.analog_gain_min_idx);
            let max_gain = (self.gain_idx + 1).min(config.analog_gain_max_idx);
            for gain_idx in min_gain..=max_gain {
                let gain_index =
                    usize::try_from(gain_idx).map_err(|_| SensorError::GainIndex(gain_idx))?;
                let gain = config.sensor_analog_gains[gain_index] * self.gain_factor();
                let time = ((desired / gain).round() as i32)
                    .clamp(config.exposure_time_min, config.exposure_time_max);
                if gain_idx < config.analog_gain_rec_idx && time > 20 && gain_idx < self.gain_idx {
                    continue;
                }
                let score = self.sensor.exposure_score(ExposureScore {
                    desired_ev: desired,
                    time,
                    gain_index: gain_idx,
                    gain,
                    previous_gain_index: self.gain_idx,
                });
                if score < self.best_ev_score {
                    self.new_exp_t = time;
                    self.new_exp_g = gain_idx;
                    self.best_ev_score = score;
                }
            }
        }
        self.measured_grey_fraction = frame.grey;
        self.target_grey_fraction = target;
        let gain_index =
            usize::try_from(self.new_exp_g).map_err(|_| SensorError::GainIndex(self.new_exp_g))?;
        self.analog_gain_frac = *config
            .sensor_analog_gains
            .get(gain_index)
            .ok_or(SensorError::GainIndex(self.new_exp_g))?;
        self.gain_idx = self.new_exp_g;
        self.exposure_time = self.new_exp_t;
        self.dc_gain_enabled = enable_dc;
        let slot = usize::try_from(frame.frame_id % 3)
            .map_err(|_| SensorError::GainIndex(self.gain_idx))?;
        self.cur_ev[slot] =
            self.exposure_time as f32 * (self.analog_gain_frac * self.gain_factor());
        Ok(Some(self.sensor.exposure_registers(Exposure {
            time: self.exposure_time,
            gain_index: self.new_exp_g,
            dc_gain: self.dc_gain_enabled,
        })?))
    }
}
