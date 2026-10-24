// Generated from unchanged PoseKalman symbolic equations by generate_pose_model.py.
pub fn observe_4(state: &[f64; 18]) -> [f64; 3] {
    let mut out = [0.0; 3];
    out[0] = state[6] + state[9];
    out[1] = state[7] + state[10];
    out[2] = state[8] + state[11];
    out
}
