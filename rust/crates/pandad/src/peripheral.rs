//! Fan and infrared policy from `process_peripheral_state` in pandad.cc.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct CameraSample {
    pub frame_id: u32,
    pub integration_lines: i32,
    pub mono_time_ns: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Input {
    pub frame: u64,
    pub now_ns: u64,
    pub fan_speed: Option<u16>,
    pub camera: Option<CameraSample>,
    pub fan_control: bool,
}

impl Default for Input {
    fn default() -> Self {
        Self {
            frame: 0,
            now_ns: 0,
            fan_speed: None,
            camera: None,
            fan_control: true,
        }
    }
}

/// Side effects are performed in source order, including the conditional Params read.
pub trait Output {
    fn driver_view_enabled(&mut self) -> bool;
    fn set_fan_speed(&mut self, speed: u16);
    fn set_panda_ir_power(&mut self, power: u16);
    fn set_hardware_ir_power(&mut self, power: i32);
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("filtered integration lines are outside the source integer range: {0}")]
    IntegrationLinesRange(f32),
}

#[derive(Debug)]
struct Filter {
    value: f32,
    coefficient: f32,
}

impl Filter {
    fn new(time_constant: f32) -> Self {
        let ratio = 0.05_f32 / time_constant;
        // The source C++ filter divides in float, then adds/divides in double.
        let coefficient = (f64::from(ratio) / (1.0 + f64::from(ratio))) as f32;
        Self {
            value: 0.0,
            coefficient,
        }
    }

    fn update(&mut self, input: i32) -> Result<i32, Error> {
        let product = self.coefficient * input as f32;
        self.value = ((1.0 - f64::from(self.coefficient)) * f64::from(self.value)
            + f64::from(product)) as f32;
        let integer = f64::from(self.value).trunc();
        if !(f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&integer) {
            return Err(Error::IntegrationLinesRange(self.value));
        }
        Ok(self.value as i32)
    }
}

#[derive(Debug)]
pub struct Peripheral {
    last_camera_ns: u64,
    previous_fan_speed: u16,
    ir_power: u16,
    previous_ir_power: u16,
    previous_frame_id: u32,
    driver_view: bool,
    normal_filter: Filter,
    driver_view_filter: Filter,
}

impl Default for Peripheral {
    fn default() -> Self {
        Self {
            last_camera_ns: 0,
            previous_fan_speed: 999,
            ir_power: 0,
            previous_ir_power: 999,
            previous_frame_id: u32::MAX,
            driver_view: false,
            normal_filter: Filter::new(30.0),
            driver_view_filter: Filter::new(5.0),
        }
    }
}

impl Peripheral {
    /// Applies one 20 Hz peripheral update using the SubMaster frame count.
    ///
    /// # Errors
    /// Returns an explicit error for a filtered value whose C++ integer cast is undefined.
    pub fn update(&mut self, input: Input, output: &mut impl Output) -> Result<(), Error> {
        let refresh = input.frame.is_multiple_of(100);
        if let Some(speed) = input.fan_speed.filter(|_| input.fan_control) {
            if speed != self.previous_fan_speed || refresh {
                output.set_fan_speed(speed);
                self.previous_fan_speed = speed;
            }
        }

        if let Some(camera) = input.camera {
            if camera.frame_id < self.previous_frame_id {
                self.normal_filter.value = 0.0;
                self.driver_view_filter.value = 0.0;
                self.driver_view = output.driver_view_enabled();
            }
            self.previous_frame_id = camera.frame_id;
            let filter = if self.driver_view {
                &mut self.driver_view_filter
            } else {
                &mut self.normal_filter
            };
            let lines = filter.update(camera.integration_lines)?;
            self.last_camera_ns = camera.mono_time_ns;
            self.ir_power = match lines {
                ..=400 => 0,
                401..=1000 => u16::try_from(100 * (lines - 400) / 600)
                    .map_err(|_| Error::IntegrationLinesRange(filter.value))?,
                1001.. => 100,
            };
        }

        if input.now_ns.wrapping_sub(self.last_camera_ns) > 1_000_000_000 {
            self.ir_power = 0;
        }
        if self.ir_power != self.previous_ir_power || refresh {
            output.set_panda_ir_power(self.ir_power / 2);
            output.set_hardware_ir_power(i32::from(self.ir_power));
            self.previous_ir_power = self.ir_power;
        }
        Ok(())
    }
}
