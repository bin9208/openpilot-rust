use super::{flags as f, Error};
use openpilot_control_policy::math::{clip, interp};

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct TorqueLimits {
    pub max: f64,
    pub delta_up: f64,
    pub delta_down: f64,
    pub allowance: f64,
    pub multiplier: f64,
    pub threshold: f64,
}

impl TorqueLimits {
    pub fn new(candidate: &str, flags: u32) -> Self {
        if flags & f::CANFD != 0 {
            return Self {
                max: 270.,
                delta_up: 2.,
                delta_down: 3.,
                allowance: 250.,
                multiplier: 2.,
                threshold: 250.,
            };
        }
        let small = matches!(
            candidate,
            "GENESIS_G80"
                | "HYUNDAI_ELANTRA"
                | "HYUNDAI_ELANTRA_GT_I30"
                | "HYUNDAI_IONIQ"
                | "HYUNDAI_IONIQ_EV_LTD"
                | "HYUNDAI_SANTA_FE_PHEV_2022"
                | "HYUNDAI_SONATA_LF"
                | "KIA_FORTE"
                | "KIA_NIRO_PHEV"
                | "KIA_OPTIMA_H"
                | "KIA_OPTIMA_H_G4_FL"
                | "KIA_SORENTO"
        );
        let alt = !small && flags & f::ALT_LIMITS != 0;
        Self {
            max: if small {
                255.
            } else if alt {
                384.
            } else {
                409.
            },
            delta_up: if alt { 2. } else { 3. },
            delta_down: if alt { 3. } else { 7. },
            allowance: 50.,
            multiplier: 2.,
            threshold: 150.,
        }
    }

    pub fn driver_limit(&self, command: f64, previous: f64, driver: f64) -> f64 {
        let driver_max = self.max + (self.allowance + driver) * self.multiplier;
        let driver_min = -self.max + (-self.allowance + driver) * self.multiplier;
        let command = clip(
            command,
            (-self.max).max(driver_min).min(0.),
            self.max.min(driver_max).max(0.),
        );
        let command = if previous > 0. {
            clip(
                command,
                (previous - self.delta_down).max(-self.delta_up),
                previous + self.delta_up,
            )
        } else {
            clip(
                command,
                previous - self.delta_up,
                (previous + self.delta_down).min(self.delta_up),
            )
        };
        command.round_ties_even()
    }
}

pub fn fault_avoidance(condition: bool, request: bool, count: &mut i32, max_frames: i32) -> bool {
    *count = if request && condition { *count + 1 } else { 0 };
    let result = request && *count <= max_frames;
    if *count >= max_frames + 2 {
        *count = 0;
    }
    result
}

pub fn jerk_limits(accel: f64, jerk: f64, error: f64) -> Result<[f64; 2], Error> {
    let up = clip(jerk * 2., 1., 5.);
    let mpc = clip(-jerk * 4., 1., 5.);
    let ratio = if jerk <= 0.1 {
        clip((error - 0.25) / 0.5, 0., 1.)
    } else {
        0.
    };
    let feedforward = interp(
        (-accel).max(0.),
        &[0., 0.8, 1.2, 1.5, 2., 2.5, 3.2],
        &[1.2, 1.2, 1.2, 1.7, 3., 3.3, 3.7],
    )?;
    Ok([up, clip(mpc.max(1. + ratio * (feedforward - 1.)), 1., 5.)])
}

#[derive(Clone, Copy, serde::Deserialize)]
pub struct AngleInput {
    pub desired: f64,
    pub previous: f64,
    pub speed: f64,
    pub measured: f64,
    pub active: bool,
    pub wheelbase: f64,
    pub ratio: f64,
    pub max_angle: f64,
    pub y_std: Option<f64>,
}

pub fn angle_limit(i: AngleInput) -> Result<f64, Error> {
    let y_std = i.y_std.filter(|v| v.is_finite() && *v >= 0.).unwrap_or(0.1);
    let mut sw_rate = interp(y_std, &[0.1, 0.2, 0.4], &[2., 1.5, 0.8])?;
    let speed = i.speed.max(1.);
    let target_sw = clip(i.desired, -i.max_angle, i.max_angle);
    if i.speed < 40. / 3.6 {
        sw_rate = sw_rate.min(interp(
            i.speed,
            &[0., 15. / 3.6, 40. / 3.6],
            &[0.8, 1.1, 0.8],
        )?);
    }
    let target_rw = target_sw / i.ratio;
    let last_rw = i.previous / i.ratio;
    let rw_max = ((8.5 * i.wheelbase) / (speed * speed)).atan().to_degrees();
    let mut rw_step = ((4. * i.wheelbase) / (speed * speed * 1.2) * 0.01).to_degrees();
    if (target_sw - i.previous).abs() > 20. {
        sw_rate = sw_rate.min(1.);
    }
    if i.previous.abs() >= 85. && i.previous * (target_sw - i.previous) < 0. {
        sw_rate = sw_rate.max(1.5);
    }
    rw_step = rw_step.min(sw_rate / i.ratio);
    let command = clip(
        clip(target_rw, last_rw - rw_step, last_rw + rw_step),
        -rw_max,
        rw_max,
    );
    Ok(clip(
        (if i.active {
            command
        } else {
            i.measured / i.ratio
        }) * i.ratio,
        -i.max_angle,
        i.max_angle,
    ))
}
