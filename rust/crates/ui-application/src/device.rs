//! ui_state.Device brightness and wakefulness; emits ordered hardware effects.
use crate::state::UiState;
use num_traits::ToPrimitive;
use serde::Serialize;
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum Effect {
    Brightness(i32),
    DisplayPower(bool),
    InteractiveTimeout,
}
#[derive(Clone, Copy)]
pub struct Input {
    pub now: f64,
    pub left_down: bool,
    pub brightness_worker_busy: bool,
    pub exposure_percent: f64,
}
#[derive(Clone, Copy)]
pub struct Config {
    pub big: bool,
    pub pc: bool,
    pub mici: bool,
    pub target_fps: i32,
}
#[derive(Serialize)]
pub struct Device {
    pub awake: bool,
    pub interaction_time: f64,
    pub override_interactive_timeout: Option<i32>,
    pub offroad_brightness: i32,
    pub last_brightness: i32,
    pub brightness_filter: f64,
    pub brightness_timer: i32,
    ignition: bool,
    previous_timed_out: bool,
    big: bool,
    pc: bool,
    default_brightness: i32,
    alpha: f64,
}
impl Device {
    pub fn new(config: Config) -> Self {
        let Config {
            big,
            pc,
            mici,
            target_fps,
        } = config;
        let brightness = if mici { 65 } else { 50 };
        let dt = 1.0 / f64::from(target_fps);
        Self {
            awake: true,
            interaction_time: -1.0,
            override_interactive_timeout: None,
            offroad_brightness: brightness,
            last_brightness: 0,
            brightness_filter: f64::from(brightness),
            brightness_timer: 20,
            ignition: false,
            previous_timed_out: false,
            big,
            pc,
            default_brightness: brightness,
            alpha: dt / (2.0 + dt),
        }
    }
    pub fn timeout(&self, ignition: bool) -> i32 {
        self.override_interactive_timeout.unwrap_or(if ignition {
            if self.big {
                10
            } else {
                5
            }
        } else {
            30
        })
    }
    pub fn set_override_timeout(&mut self, timeout: Option<i32>, now: f64, ignition: bool) {
        self.override_interactive_timeout = timeout;
        self.reset_timeout(now, ignition);
    }
    fn reset_timeout(&mut self, now: f64, ignition: bool) {
        self.interaction_time = now + f64::from(self.timeout(ignition));
    }
    pub fn set_offroad_brightness(&mut self, brightness: Option<i32>) {
        self.offroad_brightness = brightness.unwrap_or(self.default_brightness).clamp(0, 100);
    }
    pub fn update(&mut self, state: &UiState, input: Input) -> Vec<Effect> {
        if self.interaction_time <= 0.0 {
            self.reset_timeout(input.now, state.ignition);
        }
        let mut effects = Vec::new();
        // Source computes brightness using the previous awake state before the wake transition.
        let mut clipped = f64::from(self.offroad_brightness);
        if state.started && state.light_sensor >= 0.0 {
            clipped = state.light_sensor;
            clipped = if clipped <= 8.0 {
                clipped / 903.3
            } else {
                ((clipped + 16.0) / 116.0).powf(3.0)
            };
            clipped = (100.0 * clipped).clamp(10.0, 100.0);
            let ratio = state.slow.show_brightness_ratio;
            if ratio <= 0.0 {
                clipped *= (0.8 + (input.exposure_percent.clamp(0.0, 15.0) / 15.0) * (0.3 - 0.8))
                    .clamp(0.3, 0.8);
            } else if self.brightness_timer <= 0 {
                clipped *= ratio;
            } else {
                self.brightness_timer -= 1;
            }
        } else {
            self.brightness_timer = 20;
        }
        self.brightness_filter = (1.0 - self.alpha) * self.brightness_filter + self.alpha * clipped;
        // Python round is ties-to-even. Values are bounded by UI brightness parameters.
        let brightness = if self.awake {
            self.brightness_filter
                .round_ties_even()
                .to_i32()
                .unwrap_or(0)
        } else {
            0
        };
        if brightness != self.last_brightness && !input.brightness_worker_busy {
            effects.push(Effect::Brightness(brightness));
            self.last_brightness = brightness;
        }
        let ignition_off = !state.ignition && self.ignition;
        self.ignition = state.ignition;
        if ignition_off || input.left_down {
            self.reset_timeout(input.now, state.ignition);
            self.brightness_timer = 20;
        }
        let timed_out = input.now > self.interaction_time;
        if timed_out && !self.previous_timed_out {
            effects.push(Effect::InteractiveTimeout);
        }
        self.previous_timed_out = timed_out;
        let awake = state.ignition || !timed_out || self.pc;
        if awake != self.awake {
            self.awake = awake;
            effects.push(Effect::DisplayPower(awake));
        }
        effects
    }
}
