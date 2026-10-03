// Generated from unchanged PoseKalman symbolic equations by generate_pose_model.py.
pub fn observe_10(state: &[f64; 18]) -> [f64; 3] {
    let mut out = [0.0; 3];
    out[0] =
        9.81 * (state[1]).sin() - state[4] * state[8] + state[5] * state[7] + state[12] + state[15];
    out[1] = -9.81 * (state[0]).sin() * (state[1]).cos() + state[3] * state[8]
        - state[5] * state[6]
        + state[13]
        + state[16];
    out[2] = -9.81 * (state[0]).cos() * (state[1]).cos() - state[3] * state[7]
        + state[4] * state[6]
        + state[14]
        + state[17];
    out
}
