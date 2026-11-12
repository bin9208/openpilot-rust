//! Comma Pedal protocol and the Ray EV calibration. Port of carrot's
//! `opendbc/car/pedal_interceptor.py` and `hyundai/pedal_interceptor.py`
//! (StarPilot 98f84431); never a default for other vehicles.
use super::Error;
use openpilot_can::{dbc::Dbc, packer::Packer, parser::Parser, Frame};
use std::sync::Arc;

pub const DBC: &str = "pedal_interceptor";
pub const COMMAND_ADDR: u32 = 0x200;
pub const SENSOR_ADDR: u32 = 0x201;
pub const BUS: u8 = 0;
pub const RX_FREQUENCY: f64 = 50.;
pub const TX_INTERVAL: u32 = 4;
pub const MAX_RX_AGE_NS: u64 = 200_000_000;

/// The vehicle profile (`PedalInterceptorParams`).
pub struct Profile {
    pub target: fn(f64, f64, f64) -> f64,
    pub driver_thresholds: [u16; 2],
    pub rate_up: f64,
    pub rate_down: f64,
    pub safety_profile: u16,
    pub cruise_ready: (&'static str, &'static str),
    pub cruise_enabled_bit: (u32, u32),
    pub cruise_speed: (&'static str, &'static str),
    pub min_enable_speed: f32,
}

fn interp(x: f64, xs: &[f64], ys: &[f64]) -> f64 {
    if x <= xs[0] {
        return ys[0];
    }
    for i in 1..xs.len() {
        if x <= xs[i] {
            return ys[i - 1] + (ys[i] - ys[i - 1]) * (x - xs[i - 1]) / (xs[i] - xs[i - 1]);
        }
    }
    ys[ys.len() - 1]
}

const RAY_CAP: f64 = 0.70;
const RAY_CAP_SPEED_BP: [f64; 4] = [0.0, 12.0, 20.0, 25.0];
const RAY_CAP_V: [f64; 4] = [0.55, 0.55, 0.65, RAY_CAP];
const RAY_OFFSET_SPEED_BP: [f64; 8] = [0.0, 2.0, 4.0, 8.0, 12.0, 16.0, 20.0, 25.0];
const RAY_OFFSET_V: [f64; 8] = [0.08, 0.13, 0.20, 0.32, 0.42, 0.51, 0.565, 0.60];
const RAY_ACCEL_GAIN: f64 = 0.22;
const RAY_DECEL_GAIN: f64 = 0.55;
const RAY_OVERSPEED_CUTOFF: f64 = 0.5;
const RAY_TAPER_BELOW_TARGET: f64 = 0.75;

pub fn ray_target(accel: f64, v_ego: f64, set_speed: f64) -> f64 {
    if !(accel.is_finite() && v_ego.is_finite() && set_speed.is_finite()) || set_speed < 1.0 {
        return 0.0;
    }
    let speed_error = set_speed - v_ego;
    if speed_error <= -RAY_OVERSPEED_CUTOFF {
        return 0.0;
    }
    let offset = interp(v_ego, &RAY_OFFSET_SPEED_BP, &RAY_OFFSET_V);
    let cap = interp(v_ego, &RAY_CAP_SPEED_BP, &RAY_CAP_V);
    let gain = if accel < 0.0 {
        RAY_DECEL_GAIN
    } else {
        RAY_ACCEL_GAIN
    };
    let mut target = (offset + accel * gain).clamp(0.0, cap);
    if speed_error < 0.0 {
        target *= (0.65 * (1.0 + speed_error / RAY_OVERSPEED_CUTOFF)).clamp(0.0, 1.0);
    } else if speed_error < RAY_TAPER_BELOW_TARGET {
        target *= 0.65 + 0.35 * speed_error / RAY_TAPER_BELOW_TARGET;
    }
    target
}

pub const RAY: Profile = Profile {
    target: ray_target,
    driver_thresholds: [272, 513],
    rate_up: 0.02,
    rate_down: 0.06,
    safety_profile: super::flags::safety::PEDAL_INTERCEPTOR_RAY,
    cruise_ready: ("LABEL11", "CC_React"),
    cruise_enabled_bit: (0x592, 35),
    cruise_speed: ("E_EMS11", "Cruise_Limit_Target"),
    min_enable_speed: -1.0,
};

/// `PEDAL_INTERCEPTOR_PARAMS.get(candidate)`.
pub fn profile(candidate: &str) -> Option<&'static Profile> {
    (candidate == "KIA_RAY_EV").then_some(&RAY)
}

/// `get_pedal_interceptor_params(CP)`.
pub fn configured(candidate: &str, flags: u32) -> Option<&'static Profile> {
    let required = super::flags::CC_ONLY_CAR | super::flags::PEDAL_INTERCEPTOR;
    if flags & required == required {
        profile(candidate)
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug)]
pub struct State {
    pub valid: bool,
    pub state: i32,
    pub gas_pressed: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            valid: false,
            state: 5,
            gas_pressed: false,
        }
    }
}

impl State {
    pub fn update(&mut self, parser: &mut Parser, profile: &Profile) -> Result<(), Error> {
        let stamp = parser
            .states
            .get(&SENSOR_ADDR)
            .and_then(|s| s.timestamps.back().copied())
            .unwrap_or(0);
        let payload = parser.raw.get(&SENSOR_ADDR).cloned().unwrap_or_default();
        let age = i128::from(parser.last_update) - i128::from(stamp);
        self.valid = parser.can_valid()
            && stamp > 0
            && payload.len() == 6
            && (0..=i128::from(MAX_RX_AGE_NS)).contains(&age);
        self.state = if self.valid {
            parser.signal("GAS_SENSOR", "STATE")? as i32
        } else {
            5
        };
        self.gas_pressed = self.valid
            && profile
                .driver_thresholds
                .iter()
                .enumerate()
                .any(|(i, threshold)| {
                    u16::from_be_bytes([payload[i * 2], payload[i * 2 + 1]]) > *threshold
                });
        Ok(())
    }

    pub fn faulted(&self) -> bool {
        !self.valid || self.state != 0
    }
}

pub struct Interceptor {
    packer: Packer,
    profile: &'static Profile,
    gas_last: f64,
}

impl Interceptor {
    pub fn new(dbc: Arc<Dbc>, profile: &'static Profile) -> Self {
        Self {
            packer: Packer::new(dbc),
            profile,
            gas_last: 0.,
        }
    }

    pub fn update(
        &mut self,
        frame: u32,
        active: bool,
        accel: f64,
        v_ego: f64,
        set_speed: f64,
    ) -> Result<Option<Frame>, Error> {
        if frame % TX_INTERVAL != 0 {
            return Ok(None);
        }
        if active {
            let target = (self.profile.target)(accel, v_ego, set_speed);
            if target <= 0.001 {
                self.gas_last = 0.;
            } else {
                let next = (self.gas_last - self.profile.rate_down)
                    .max(target.min(self.gas_last + self.profile.rate_up));
                self.gas_last = next.min(target);
            }
        } else {
            self.gas_last = 0.;
        }
        let enabled = self.gas_last > 0.001;
        let counter = f64::from((frame / TX_INTERVAL) & 0xf);
        let mut signals = vec![
            ("ENABLE", if enabled { 1. } else { 0. }),
            ("COUNTER_PEDAL", counter),
        ];
        // Disabled tracks stay unset so the packer emits raw zero.
        if enabled {
            signals.push(("GAS_COMMAND", self.gas_last * 255.));
            signals.push(("GAS_COMMAND2", self.gas_last * 255.));
        }
        Ok(Some(Frame {
            address: COMMAND_ADDR,
            data: self.packer.pack("GAS_COMMAND", &signals, None)?,
            bus: BUS,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_target_tapers_and_cuts_overspeed() {
        assert_eq!(ray_target(0., 10., 0.5), 0.);
        assert_eq!(ray_target(0., 10., 9.4), 0.);
        let cruise = ray_target(0., 10., 12.);
        assert!((cruise - interp(10., &RAY_OFFSET_SPEED_BP, &RAY_OFFSET_V)).abs() < 1e-9);
        assert!(ray_target(5., 10., 12.) <= interp(10., &RAY_CAP_SPEED_BP, &RAY_CAP_V));
        assert!(ray_target(0., 10., 10.2) < cruise);
    }

    #[test]
    fn command_packs_with_pedal_checksum() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../opendbc_repo/opendbc/dbc/pedal_interceptor.dbc"
        );
        let dbc = Arc::new(Dbc::load(std::path::Path::new(path)).unwrap());
        let mut pedal = Interceptor::new(dbc, &RAY);
        assert!(pedal.update(1, true, 0., 10., 12.).unwrap().is_none());
        let frame = pedal.update(4, false, 0., 10., 12.).unwrap().unwrap();
        assert_eq!(frame.address, 0x200);
        // CRC8 0xD5 over the first five bytes in reverse order.
        let mut crc: u8 = 0xff;
        for b in frame.data[..5].iter().rev() {
            crc ^= b;
            for _ in 0..8 {
                crc = (crc << 1) ^ if crc & 0x80 != 0 { 0xd5 } else { 0 };
            }
        }
        assert_eq!(frame.data[5], crc);
        let ramp = pedal.update(8, true, 0., 10., 12.).unwrap().unwrap();
        assert_ne!(ramp.data[..4], [0, 0, 0, 0], "first ramp step is nonzero");
    }
}
