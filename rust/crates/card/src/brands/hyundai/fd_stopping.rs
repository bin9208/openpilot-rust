use super::{
    canfd_acc::{AccInput, SccState},
    stopping::{CanfdStopping, StopInput, MOVING_SPEED, STOP_LOWER_BAND},
    wire::{get, set, Values},
    Error,
};
use num_traits::ToPrimitive;
use openpilot_control_policy::math::clip;

pub fn apply_stopping(
    data: &mut Values,
    state: &SccState<'_>,
    context: (Option<&mut CanfdStopping>, &AccInput, f64, bool),
) -> Result<(), Error> {
    let (controller, input, jerk_u, numpy_value) = context;
    let Some(controller) = controller else {
        return Ok(());
    };
    let mut speeds = [state.v_ego, state.v_ego_raw, 0., 0., 0., 0.];
    speeds[2..].copy_from_slice(&state.wheels);
    let finite = speeds
        .iter()
        .chain(
            [
                state.a_ego,
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
    let previous_phase = controller.phase;
    let command = controller.update(StopInput {
        active: mode == 1. && !blocked,
        requested: get(data, "StopReq")? != 0.,
        speed,
        held: state.scc_hold,
        accel: input.accel,
        previous_value: input.value_last,
        jerk_u: clip(jerk_u, 0., 5.),
        jerk_l: clip(input.jerk_l, 1., 5.),
    });
    if blocked || mode != 1. {
        set(
            data,
            &[
                ("StopReq", 0.),
                ("aReqRaw", 0.),
                ("aReqValue", 0.),
                ("AccelLimitBandLower", 0.),
            ],
        );
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
    } else {
        data.insert("AccelLimitBandLower".into(), STOP_LOWER_BAND);
    }
    let stop_req = get(data, "StopReq")?.to_u8().ok_or(Error::Numeric)?;
    let jerk = controller.limit_scc_jerk_upper(stop_req, get(data, "JerkUpperLimit")?);
    data.insert("JerkUpperLimit".into(), jerk);
    let value_text = source_float(get(data, "aReqValue")?);
    // The non-camera source builder retains its NumPy scalar only when the
    // stopping policy leaves the ordinary active packet unchanged.
    let value_text = if numpy_value && !blocked && mode == 1. && command.is_none() {
        format!("np.float64({value_text})")
    } else {
        value_text
    };
    if controller.phase != previous_phase {
        controller.warnings.push(format!(
            "{{'event': 'carrot_stopping', 'from': '{}', 'phase': '{}', 'reason': '{}', 'speed': {}, 'aEgo': {}, 'held': {}, 'soft_hold': {}, 'StopReq': {}, 'aReqRaw': {}, 'aReqValue': {}}}",
            previous_phase.as_str(),controller.phase.as_str(),controller.reason,
            source_float(speed),source_float(state.a_ego),source_bool(state.scc_hold),source_bool(soft_hold),
            stop_req,source_float(get(data,"aReqRaw")?),value_text,
        ));
    }
    Ok(())
}
fn source_bool(value: bool) -> &'static str {
    if value {
        "True"
    } else {
        "False"
    }
}
fn source_float(value: f64) -> String {
    if value.is_nan() {
        return "nan".into();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-inf".into()
        } else {
            "inf".into()
        };
    }
    let mut text = String::new();
    openpilot_runtime_core::python_float::write_float(value, &mut text).expect("String formatting");
    text
}
