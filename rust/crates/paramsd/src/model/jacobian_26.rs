// Generated from unchanged CarKalman symbolic equations by generate_car_model.py.
pub fn jacobian_26(state: &[f64; 9]) -> [f64; 9] {
    let _ = state;
    let mut out = [0.0; 9];
    out[7] = 1.0;
    out
}
