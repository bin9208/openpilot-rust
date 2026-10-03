use crate::model;

fn state(values: &[f64]) -> [f64; 9] {
    let mut state = [0.0; 9];
    state.copy_from_slice(values);
    state
}
pub fn model_transition(values: &[f64], globals: &[f64], dt: f64, out: &mut [f64]) {
    out.copy_from_slice(&model::transition(
        &state(values),
        &globals.try_into().expect("validated globals"),
        dt,
    ));
}
pub fn model_transition_jacobian(values: &[f64], globals: &[f64], dt: f64, out: &mut [f64]) {
    out.copy_from_slice(&model::transition_jacobian(
        &state(values),
        &globals.try_into().expect("validated globals"),
        dt,
    ));
}
pub fn model_observe(kind: i32, values: &[f64], out: &mut [f64]) {
    let state = state(values);
    match kind {
        24 => out.copy_from_slice(&model::observe_24(&state)),
        25 => out.copy_from_slice(&model::observe_25(&state)),
        26 => out.copy_from_slice(&model::observe_26(&state)),
        27 => out.copy_from_slice(&model::observe_27(&state)),
        28 => out.copy_from_slice(&model::observe_28(&state)),
        29 => out.copy_from_slice(&model::observe_29(&state)),
        30 => out.copy_from_slice(&model::observe_30(&state)),
        31 => out.copy_from_slice(&model::observe_31(&state)),
        _ => unreachable!("validated observation kind"),
    }
}
pub fn model_observe_jacobian(kind: i32, values: &[f64], out: &mut [f64]) {
    let state = state(values);
    match kind {
        24 => out.copy_from_slice(&model::jacobian_24(&state)),
        25 => out.copy_from_slice(&model::jacobian_25(&state)),
        26 => out.copy_from_slice(&model::jacobian_26(&state)),
        27 => out.copy_from_slice(&model::jacobian_27(&state)),
        28 => out.copy_from_slice(&model::jacobian_28(&state)),
        29 => out.copy_from_slice(&model::jacobian_29(&state)),
        30 => out.copy_from_slice(&model::jacobian_30(&state)),
        31 => out.copy_from_slice(&model::jacobian_31(&state)),
        _ => unreachable!("validated observation kind"),
    }
}
pub fn model_error(values: &[f64], delta: &[f64], out: &mut [f64]) {
    assert_eq!((values.len(), delta.len(), out.len()), (9, 9, 9));
    for i in 0..9 {
        out[i] = values[i] + delta[i];
    }
}
pub fn model_identity(out: &mut [f64]) {
    assert_eq!(out.len(), 81);
    out.fill(0.0);
    for i in 0..9 {
        out[i * 9 + i] = 1.0;
    }
}
