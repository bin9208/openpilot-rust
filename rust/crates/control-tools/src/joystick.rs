use crate::Error;
use openpilot_control_policy::math::{clip, interp, maximum, minimum};
use serde::Deserialize;

#[derive(Default)]
pub struct Keyboard {
    pub axes: [f64; 2],
    pub cancel: bool,
}

impl Keyboard {
    pub fn update(&mut self, key: &str) -> bool {
        self.cancel = false;
        match key.to_lowercase().as_str() {
            "r" => self.axes = [0.0; 2],
            "c" => self.cancel = true,
            "w" => self.axes[0] = clip(self.axes[0] + 0.05, -1.0, 1.0),
            "s" => self.axes[0] = clip(self.axes[0] - 0.05, -1.0, 1.0),
            "a" => self.axes[1] = clip(self.axes[1] + 0.05, -1.0, 1.0),
            "d" => self.axes[1] = clip(self.axes[1] - 0.05, -1.0, 1.0),
            _ => return false,
        }
        true
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    Pc,
    Tici,
}

pub struct Gamepad {
    profile: Profile,
    pub axes: [f64; 2],
    pub minimum: [f64; 2],
    pub maximum: [f64; 2],
    pub cancel: bool,
}

impl Gamepad {
    pub const fn new(profile: Profile) -> Self {
        Self {
            profile,
            axes: [0.0; 2],
            minimum: [0.0; 2],
            maximum: [255.0; 2],
            cancel: false,
        }
    }

    pub const fn names(&self) -> [&'static str; 2] {
        match self.profile {
            Profile::Pc => ["ABS_Z", "ABS_RX"],
            Profile::Tici => ["ABS_RX", "ABS_Z"],
        }
    }

    pub fn update(&mut self, code: &str, state: i32) -> Result<bool, Error> {
        let names = self.names();
        let brake = match self.profile {
            Profile::Pc => "ABS_RZ",
            Profile::Tici => "ABS_RY",
        };
        let (code, value) = if code == brake {
            (names[0], -f64::from(state))
        } else {
            (code, f64::from(state))
        };
        if code == "BTN_NORTH" {
            match state {
                1 => self.cancel = true,
                0 => self.cancel = false,
                _ => (),
            }
        } else if let Some(index) = names.iter().position(|name| *name == code) {
            self.maximum[index] = maximum(value, self.maximum[index]);
            self.minimum[index] = minimum(value, self.minimum[index]);
            let normalized = -interp(
                value,
                &[self.minimum[index], self.maximum[index]],
                &[-1.0, 1.0],
            )?;
            let normalized = if normalized.abs() > 0.03 {
                normalized
            } else {
                0.0
            };
            self.axes[index] = 0.4 * normalized.powf(3.0) + (1.0 - 0.4) * normalized;
        } else {
            return Ok(false);
        }
        Ok(true)
    }

    pub fn disconnected(&mut self) {
        self.axes = [0.0; 2];
    }
}
