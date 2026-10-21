// Generated from unchanged CarKalman symbolic equations by generate_car_model.py.
pub fn observe_30(state: &[f64; 9]) -> [f64; 1] {
    let mut out = [0.0; 1];
    out[0] = state[4];
    out
}
