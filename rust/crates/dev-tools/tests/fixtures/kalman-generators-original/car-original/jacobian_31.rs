// Generated from unchanged CarKalman symbolic equations by generate_car_model.py.
pub fn jacobian_31(state: &[f64; 9]) -> [f64; 9] {
    let _ = state;
    let mut out = [0.0; 9];
    out[8] = 1.0;
    out
}
