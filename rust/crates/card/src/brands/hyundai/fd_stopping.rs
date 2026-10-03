use super::{
    canfd_acc::{AccInput, SccState},
    stopping::{CanfdStopping, StopInput, MOVING_SPEED},
    wire::{get, set, Values},
    Error,
};
use openpilot_control_policy::math::clip;

pub fn apply_stopping(
    data: &mut Values,
    state: &SccState<'_>,
    context: (Option<&mut CanfdStopping>, &AccInput, f64),
) -> Result<(), Error> {
    let (controller, input, jerk_u) = context;
    let Some(controller) = controller else {
        return Ok(());
    };
    let mut speeds = [state.v_ego, state.v_ego_raw, 0., 0., 0., 0.];
    speeds[2..].copy_from_slice(&state.wheels);
    let finite = speeds
        .iter()
        .chain(
            [
                input.accel,
                get(data, "aReqValue")?,
                input.value_last,
                jerk_u,
                input.jerk_l,
            ]
            .iter(),
        )
        .all(|v| v.is_finite());
    let speed = if finite {
        speeds.iter().map(|v| v.abs()).fold(0., f64::max)
    } else {
        0.
    };
    let soft_hold = state.soft_hold && state.available;
    let brake_blocked = state.brake && !(soft_hold && speed <= MOVING_SPEED);
    let blocked = !finite
        || !state.can_valid
        || brake_blocked
        || state.gas
        || !state.drive
        || state.brake_hold
        || state.parking_brake;
    let mode = get(data, "ACCMode")?;
    let command = controller.update(StopInput {
        active: mode == 1. && !blocked,
        requested: get(data, "StopReq")? != 0.,
        speed,
        held: state.scc_hold,
        accel: input.accel,
        value: get(data, "aReqValue")?,
        previous_value: input.value_last,
        jerk_u: clip(jerk_u, 0., 5.),
        jerk_l: clip(input.jerk_l, 1., 5.),
    });
    if blocked || mode != 1. {
        set(data, &[("StopReq", 0.), ("aReqRaw", 0.), ("aReqValue", 0.)]);
        if !finite {
            set(
                data,
                &[
                    ("ACCMode", 0.),
                    ("JerkUpperLimit", 1.),
                    ("JerkLowerLimit", 1.),
                ],
            );
        }
    } else if let Some(command) = command {
        set(
            data,
            &[
                ("StopReq", f64::from(command.stop_req)),
                ("aReqRaw", command.raw),
                ("aReqValue", command.value),
                ("AccelLimitBandUpper", 0.),
                ("AccelLimitBandLower", command.lower),
            ],
        );
    }
    Ok(())
}
