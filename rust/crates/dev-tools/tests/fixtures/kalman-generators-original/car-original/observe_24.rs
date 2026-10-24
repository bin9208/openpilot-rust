// Generated from unchanged CarKalman symbolic equations by generate_car_model.py.
pub fn observe_24(state: &[f64; 9]) -> [f64; 2] {
    let mut out = [0.0; 2];
    out[0] = state[4];
    out[1] = state[5];
    out
}
