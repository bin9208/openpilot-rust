use super::gamepad::{DualSenseState, GamepadSample};
use crate::models::SimulatorInput;
use openpilot_sdl2::{Context, Device, DeviceKind};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum GamepadError {
    #[error("No gamepad found. Use --input random to run without a controller.")]
    Missing,
    #[error(transparent)]
    Sdl(#[from] openpilot_sdl2::Error),
}

pub struct DualSenseSimulator {
    device: Option<Device>,
    state: DualSenseState,
}

impl DualSenseSimulator {
    /// # Errors
    /// Returns initialization, missing gamepad, or device-open errors.
    pub fn new(controller_index: i32, now: f64) -> Result<Self, GamepadError> {
        Self::open_with_context(
            &Context::load(Path::new("libSDL2-2.0.so.0"))?,
            controller_index,
            now,
        )
    }
    /// # Errors
    /// Returns an error if no gamepad exists or the selected index cannot open.
    pub fn open_with_context(
        context: &Context,
        controller_index: i32,
        now: f64,
    ) -> Result<Self, GamepadError> {
        if context.device_count() == 0 {
            return Err(GamepadError::Missing);
        }
        Ok(Self {
            device: Some(context.open(controller_index)?),
            state: DualSenseState::new(now),
        })
    }
    /// # Errors
    /// Returns an error after the input source has been closed.
    pub fn read_input(&mut self, now: f64) -> Result<SimulatorInput, GamepadError> {
        let snapshot = self
            .device
            .as_mut()
            .ok_or(openpilot_sdl2::Error::Closed)?
            .snapshot()?;
        let sample = match snapshot.kind {
            DeviceKind::Controller => GamepadSample::Controller {
                axes: snapshot.axes,
                buttons: snapshot.buttons,
            },
            DeviceKind::Joystick => GamepadSample::Joystick {
                axes: snapshot.axes,
                buttons: snapshot.buttons,
                hat_x: if snapshot.hat & 2 != 0 {
                    1
                } else if snapshot.hat & 8 != 0 {
                    -1
                } else {
                    0
                },
            },
        };
        Ok(self.state.read_sample(&sample, now))
    }
    #[must_use]
    pub fn status_text(&self) -> String {
        self.state.status_text()
    }
    pub fn close(&mut self) {
        self.device = None;
    }
}
