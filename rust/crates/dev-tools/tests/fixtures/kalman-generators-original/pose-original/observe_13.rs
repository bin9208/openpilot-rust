// Generated from unchanged PoseKalman symbolic equations by generate_pose_model.py.
pub fn observe_13(state: &[f64; 18]) -> [f64; 3] {
    let mut out = [0.0; 3];
    out[0] = state[3];
    out[1] = state[4];
    out[2] = state[5];
    out
}
