use super::clamp;
pub use super::native_gamepad::{DualSenseSimulator, GamepadError};
use crate::models::SimulatorInput;

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum GamepadSample {
    Controller {
        axes: Vec<i16>,
        buttons: Vec<bool>,
    },
    Joystick {
        axes: Vec<i16>,
        buttons: Vec<bool>,
        hat_x: i8,
    },
}
#[must_use]
pub fn normalize_signed_axis(value: f64) -> f64 {
    let value = if !(-1.0..=1.0).contains(&value) && !value.is_nan() {
        if value >= 0.0 {
            value / 32767.0
        } else {
            value / 32768.0
        }
    } else {
        value
    };
    clamp(value, -1.0, 1.0)
}
#[must_use]
pub fn normalize_trigger_axis(value: f64) -> f64 {
    let value = if !(-1.0..=1.0).contains(&value) && !value.is_nan() {
        if value < 0.0 {
            (value + 32768.0) / 65535.0
        } else {
            value / 32767.0
        }
    } else if value < 0.0 {
        (value + 1.0) * 0.5
    } else {
        value
    };
    if value < 0.03 {
        0.0
    } else {
        clamp(value, 0.0, 1.0)
    }
}
#[must_use]
pub fn normalize_stick(value: f64) -> f64 {
    if value.abs() < 0.06 {
        0.0
    } else {
        clamp(value, -1.0, 1.0)
    }
}
#[must_use]
pub fn normalize_view_axis(value: f64) -> f64 {
    if value.abs() < 0.08 {
        0.0
    } else {
        clamp(value, -1.0, 1.0)
    }
}
#[derive(Clone, Debug)]
pub struct DualSenseState {
    warmup_until: f64,
    command: SimulatorInput,
    view_rotate_x: f64,
    view_rotate_y: f64,
}
impl DualSenseState {
    #[must_use]
    pub fn new(now: f64) -> Self {
        Self {
            warmup_until: now + 0.6,
            command: SimulatorInput::default(),
            view_rotate_x: 0.0,
            view_rotate_y: 0.0,
        }
    }
    pub fn read_sample(&mut self, sample: &GamepadSample, now: f64) -> SimulatorInput {
        let (axes, buttons, controller, hat) = match sample {
            GamepadSample::Controller { axes, buttons } => (axes, buttons, true, 0),
            GamepadSample::Joystick {
                axes,
                buttons,
                hat_x,
            } => (axes, buttons, false, *hat_x),
        };
        let read_axis = |index: usize| {
            let value = f64::from(axes.get(index).copied().unwrap_or(0));
            if controller {
                value
            } else {
                value / 32768.0
            }
        };
        let button = |index: usize| buttons.get(index).copied().unwrap_or(false);
        let steering = normalize_stick(normalize_signed_axis(read_axis(0)));
        let brake = normalize_trigger_axis(read_axis(4));
        let throttle = normalize_trigger_axis(read_axis(5));
        self.view_rotate_x = normalize_view_axis(normalize_signed_axis(read_axis(2)));
        self.view_rotate_y = normalize_view_axis(normalize_signed_axis(read_axis(3)));
        let (left, right) = if controller {
            (button(13) || button(9), button(14) || button(10))
        } else {
            (
                [4, 9, 13].into_iter().any(button) || hat < 0,
                [5, 10, 14].into_iter().any(button) || hat > 0,
            )
        };
        self.command = SimulatorInput {
            throttle,
            brake,
            steering,
            surround_yaw_deg: self.view_rotate_x * 180.0,
            surround_pitch_deg: -self.view_rotate_y * 18.0,
            surround_view_active: self.view_rotate_x != 0.0 || self.view_rotate_y != 0.0,
            left_signal_requested: left,
            right_signal_requested: right,
            ..SimulatorInput::default()
        };
        if now < self.warmup_until {
            self.view_rotate_x = 0.0;
            self.view_rotate_y = 0.0;
            self.command = SimulatorInput {
                surround_pitch_deg: -0.0,
                ..SimulatorInput::default()
            };
        }
        self.command.clone()
    }
    #[must_use]
    pub fn status_text(&self) -> String {
        format!(
            "R2={:.2} L2={:.2} LSX={:+.2} RS={:+.2},{:+.2} SIG={}{}",
            self.command.throttle,
            self.command.brake,
            self.command.steering,
            self.view_rotate_x,
            self.view_rotate_y,
            if self.command.left_signal_requested {
                "L"
            } else {
                "-"
            },
            if self.command.right_signal_requested {
                "R"
            } else {
                "-"
            }
        )
    }
}
