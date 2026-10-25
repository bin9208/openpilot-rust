// Generated from unchanged CarKalman symbolic equations by generate_car_model.py.
pub fn jacobian_24(state: &[f64; 9]) -> [f64; 18] {
    let _ = state;
    let mut out = [0.0; 18];
    out[4] = 1.0;
    out[14] = 1.0;
    out
}
