use super::{integer, model::Model, Error};
use openpilot_control_policy::math::{clip, interp, maximum};
use serde::Serialize;

#[derive(Serialize)]
pub struct Limits {
    pub steer_max: i32,
    pub steer_delta_up: i32,
    pub steer_delta_down: i32,
    pub max_gas: f64,
    pub max_acc_regen: f64,
    pub inactive_regen: i32,
    pub gas_lookup_bp: [f64; 3],
    pub gas_lookup_v: [f64; 3],
    pub brake_lookup_bp: [f64; 2],
    pub ev_gas_lookup_bp: Option<[f64; 3]>,
    pub ev_brake_lookup_bp: Option<[f64; 2]>,
}
impl Limits {
    pub fn new(model: &Model) -> Self {
        let camera_acc = (model.camera || model.sdgm) && !model.cc;
        let max_gas = if camera_acc { 1346. } else { 1018. };
        let max_acc_regen = if camera_acc { -540. } else { -650. };
        let threshold = if camera_acc {
            0.
        } else if model.ev {
            -1.
        } else {
            -0.1
        };
        Self {
            steer_max: 300,
            steer_delta_up: 5,
            steer_delta_down: 7,
            max_gas,
            max_acc_regen,
            inactive_regen: if camera_acc { -500 } else { -650 },
            gas_lookup_bp: [threshold, 0., 2.],
            gas_lookup_v: [max_acc_regen, 0., max_gas],
            brake_lookup_bp: [-4., threshold],
            ev_gas_lookup_bp: None,
            ev_brake_lookup_bp: None,
        }
    }
    pub fn update_ev(&mut self, speed: f64) -> Result<(), Error> {
        let threshold = interp(
            speed,
            &[
                1.29, 1.52, 1.55, 1.6, 1.7, 1.8, 2., 2.2, 2.5, 5.52, 9.6, 20.5, 23.5, 35.,
            ],
            &[
                0., -0.14, -0.16, -0.18, -0.215, -0.255, -0.32, -0.41, -0.5, -0.72, -0.905, -1.14,
                -1.16, -1.175,
            ],
        )?;
        self.ev_gas_lookup_bp = Some([threshold, maximum(0., threshold), 2.]);
        self.ev_brake_lookup_bp = Some([-4., threshold]);
        Ok(())
    }
    pub fn torque(&self, command: f64, previous: i32, driver: f32) -> Result<i32, Error> {
        if !command.is_finite() {
            return Err(Error::Numeric);
        }
        let max = f64::from(self.steer_max);
        let driver_max = max + (65. + f64::from(driver) * 100.) * 4.;
        let driver_min = -max + (-65. + f64::from(driver) * 100.) * 4.;
        let command = clip(
            command,
            (-max).max(driver_min).min(0.),
            max.min(driver_max).max(0.),
        );
        let previous = f64::from(previous);
        let up = f64::from(self.steer_delta_up);
        let down = f64::from(self.steer_delta_down);
        let command = if previous > 0. {
            clip(command, (previous - down).max(-up), previous + up)
        } else {
            clip(command, previous - up, (previous + down).min(up))
        };
        integer(command.round_ties_even())
    }
}
pub fn pedal(accel: f64, active: bool, velocity: f64) -> Result<(f64, bool), Error> {
    if !active {
        return Ok((0., false));
    }
    if accel < -0.3 {
        return Ok((0., true));
    }
    let offset = interp(velocity, &[0., 3., 6., 30.], &[0.08, 0.175, 0.240, 0.240])?;
    let gas = clip(offset + accel * 0.6, 0., 1.);
    let maximum = interp(velocity, &[0., 5., 30.], &[0.21, 0.3175, 0.3525])?;
    Ok((clip(gas, 0., maximum), false))
}
