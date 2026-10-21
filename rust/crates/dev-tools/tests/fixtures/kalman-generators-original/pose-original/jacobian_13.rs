// Generated from unchanged PoseKalman symbolic equations by generate_pose_model.py.
pub fn jacobian_13(state: &[f64; 18]) -> [f64; 54] {
    let _ = state;
    let mut out = [0.0; 54];
    out[3] = 1.0;
    out[22] = 1.0;
    out[41] = 1.0;
    out
}
