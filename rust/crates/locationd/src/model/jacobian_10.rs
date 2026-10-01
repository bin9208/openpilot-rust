// Generated from unchanged PoseKalman symbolic equations by generate_pose_model.py.
pub fn jacobian_10(state: &[f64; 18]) -> [f64; 54] {
    let mut out = [0.0; 54];
    out[1] = 9.81 * (state[1]).cos();
    out[4] = -state[8];
    out[5] = state[7];
    out[7] = state[5];
    out[8] = -state[4];
    out[12] = 1.0;
    out[15] = 1.0;
    out[18] = -9.81 * (state[0]).cos() * (state[1]).cos();
    out[19] = 9.81 * (state[0]).sin() * (state[1]).sin();
    out[21] = state[8];
    out[23] = -state[6];
    out[24] = -state[5];
    out[26] = state[3];
    out[31] = 1.0;
    out[34] = 1.0;
    out[36] = 9.81 * (state[0]).sin() * (state[1]).cos();
    out[37] = 9.81 * (state[1]).sin() * (state[0]).cos();
    out[39] = -state[7];
    out[40] = state[6];
    out[42] = state[4];
    out[43] = -state[3];
    out[50] = 1.0;
    out[53] = 1.0;
    out
}
