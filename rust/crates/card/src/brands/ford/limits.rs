use super::Error;
use openpilot_control_policy::math::{clip, interp};
pub fn curvature_limit(
    mut command: f64,
    previous: f64,
    current: f64,
    speed: f64,
    active: bool,
    canfd: bool,
) -> Result<f64, Error> {
    if speed > 9. {
        command = clip(command, current - 0.002, current + 0.002);
    }
    let increasing = previous * command >= 0. && command.abs() > previous.abs();
    let rate = interp(
        speed,
        &[5., 25.],
        if increasing {
            &[0.00045, 0.0001]
        } else {
            &[0.00045, 0.00015]
        },
    )?;
    command = clip(command, previous - rate, previous + rate);
    if !active {
        command = 0.;
    }
    command = clip(command, -0.02, 0.02);
    if canfd {
        let limit = (3. - 9.81 * 0.06) / speed.max(1.).powi(2);
        command = clip(command, -limit, limit);
    }
    Ok(command)
}
