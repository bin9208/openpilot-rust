// Generated from unchanged PoseKalman symbolic equations by generate_pose_model.py.
pub fn jacobian_14(state: &[f64; 18]) -> [f64; 54] {
    let _ = state;
    let mut out = [0.0; 54];
    out[6] = 1.0;
    out[25] = 1.0;
    out[44] = 1.0;
    out
}
