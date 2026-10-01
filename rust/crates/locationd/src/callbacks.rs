use crate::model;

fn state(values: &[f64]) -> [f64; 18] {
    let mut state = [0.0; 18];
    state.copy_from_slice(values);
    state
}
pub fn model_transition(values: &[f64], dt: f64, out: &mut [f64]) {
    out.copy_from_slice(&model::transition(&state(values), dt));
}
pub fn model_transition_jacobian(values: &[f64], dt: f64, out: &mut [f64]) {
    out.copy_from_slice(&model::transition_jacobian(&state(values), dt));
}
pub fn model_observe(kind: i32, values: &[f64], out: &mut [f64]) {
    let state = state(values);
    let result = match kind {
        4 => model::observe_4(&state),
        10 => model::observe_10(&state),
        13 => model::observe_13(&state),
        14 => model::observe_14(&state),
        _ => unreachable!("native callback kind is validated before rednose entry"),
    };
    out.copy_from_slice(&result);
}
pub fn model_observe_jacobian(kind: i32, values: &[f64], out: &mut [f64]) {
    let state = state(values);
    let result = match kind {
        4 => model::jacobian_4(&state),
        10 => model::jacobian_10(&state),
        13 => model::jacobian_13(&state),
        14 => model::jacobian_14(&state),
        _ => unreachable!("native callback kind is validated before rednose entry"),
    };
    out.copy_from_slice(&result);
}
pub fn model_error(values: &[f64], delta: &[f64], out: &mut [f64]) {
    assert_eq!((values.len(), delta.len(), out.len()), (18, 18, 18));
    for i in 0..18 {
        out[i] = values[i] + delta[i];
    }
}
pub fn model_identity(out: &mut [f64]) {
    assert_eq!(out.len(), 324);
    out.fill(0.0);
    for i in 0..18 {
        out[i * 18 + i] = 1.0;
    }
}
