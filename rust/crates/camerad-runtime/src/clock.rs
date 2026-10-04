use crate::{CameraError, FrameClock};
use openpilot_camera_kernel::{parse_double_prefix, random_unit};
use openpilot_camerad::requests::{Diagnostic, StressPoint};
use std::{ffi::CString, time::Duration};

struct StressConfig {
    probability: f64,
    interval: f64,
}

pub struct SystemClock {
    stress: Option<StressConfig>,
    last_trigger: f64,
    camera: usize,
    boottime: fn() -> (i64, i64),
}

fn boottime() -> (i64, i64) {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::Boottime);
    (time.tv_sec, time.tv_nsec)
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::with_boottime(boottime)
    }
}

impl SystemClock {
    pub fn with_boottime(boottime: fn() -> (i64, i64)) -> Self {
        Self {
            stress: None,
            last_trigger: 0.0,
            camera: 0,
            boottime,
        }
    }
    pub fn set_camera(&mut self, camera: usize) {
        self.camera = camera;
    }

    fn setting(variable: &'static str, default: &str) -> Result<f64, CameraError> {
        let value = std::env::var_os(variable).unwrap_or_else(|| default.into());
        let value = CString::new(value.as_encoded_bytes())
            .map_err(|_| CameraError::StressEncoding(variable))?;
        parse_double_prefix(&value).map_err(|source| CameraError::StressValue { variable, source })
    }

    fn milliseconds(&self) -> f64 {
        let (seconds, nanos) = (self.boottime)();
        let seconds = seconds as f64;
        let nanos = nanos as f64;
        if cfg!(target_arch = "aarch64") {
            seconds.mul_add(1000.0, nanos * 1e-6)
        } else {
            seconds * 1000.0 + nanos * 1e-6
        }
    }
}

impl FrameClock for SystemClock {
    fn diagnostic(&mut self, value: Diagnostic<'_>) {
        crate::diagnostics::request(value);
    }
    fn now_ms(&mut self) -> f64 {
        self.milliseconds()
    }
    fn now_ns(&mut self) -> u64 {
        let (seconds, nanos) = (self.boottime)();
        seconds as u64 * 1_000_000_000 + nanos as u64
    }

    fn stress(&mut self, point: StressPoint) -> Result<bool, CameraError> {
        if self.stress.is_none() {
            self.stress = Some(StressConfig {
                probability: Self::setting("SPECTRA_ERROR_PROB", "-1")?,
                interval: Self::setting("SPECTRA_ERROR_DT", "1")?,
            });
        }
        let config = self.stress.as_ref().ok_or(CameraError::Closed)?;
        let triggered = config.probability > 0.0
            && random_unit() < config.probability
            && self.milliseconds() - self.last_trigger > config.interval;
        if triggered {
            self.last_trigger = self.milliseconds();
            camera_log!(Error, "stress test (cam {}): {}", self.camera, point.name());
        }
        Ok(triggered)
    }

    fn sleep_ms(&mut self, millis: u64) {
        std::thread::sleep(Duration::from_millis(millis));
    }
}
