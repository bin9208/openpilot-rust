use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Default, Deserialize)]
pub struct VehicleSnapshot {
    pub device_alive: bool,
    pub started: bool,
    pub car_alive: bool,
    pub car_valid: bool,
    pub can_valid: bool,
    pub controls_alive: bool,
    pub enabled: bool,
    pub brake: bool,
    pub gas: bool,
    pub gear_drive: bool,
    pub physical_buttons: bool,
    pub v_ego: f64,
}

#[derive(Clone, Copy, Default, Serialize)]
pub struct Gates {
    pub started: bool,
    pub car_ok: bool,
    pub hold_blocked: bool,
    pub stationary: bool,
    enabled: bool,
}

impl Gates {
    pub fn update(&mut self, state: VehicleSnapshot) {
        self.started = state.device_alive && state.started;
        self.car_ok = state.car_alive && state.car_valid && state.can_valid;
        let enabled = state.controls_alive && state.enabled;
        self.hold_blocked = !self.started
            || !self.car_ok
            || !state.controls_alive
            || state.brake
            || state.gas
            || !state.gear_drive
            || state.physical_buttons
            || (self.enabled && !enabled);
        self.enabled = enabled;
        self.stationary = (state.device_alive && !state.started)
            || (self.car_ok && state.v_ego.abs() < 0.1 && state.controls_alive && !state.enabled);
    }
}
