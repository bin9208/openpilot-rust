// Generated from unchanged PoseKalman symbolic equations by generate_pose_model.py.
pub fn transition(state: &[f64; 18], dt: f64) -> [f64; 18] {
    let mut out = [0.0; 18];
    out[0] = (((dt * state[6]).sin() * (dt * state[7]).sin() * (dt * state[8]).sin()
        + (dt * state[6]).cos() * (dt * state[8]).cos())
        * (state[0]).sin()
        * (state[1]).cos()
        - ((dt * state[6]).sin() * (dt * state[7]).sin() * (dt * state[8]).cos()
            - (dt * state[8]).sin() * (dt * state[6]).cos())
            * (state[1]).sin()
        + (dt * state[6]).sin() * (dt * state[7]).cos() * (state[0]).cos() * (state[1]).cos())
    .atan2(
        -((dt * state[6]).sin() * (dt * state[8]).sin()
            + (dt * state[7]).sin() * (dt * state[6]).cos() * (dt * state[8]).cos())
            * (state[1]).sin()
            + (-(dt * state[6]).sin() * (dt * state[8]).cos()
                + (dt * state[7]).sin() * (dt * state[8]).sin() * (dt * state[6]).cos())
                * (state[0]).sin()
                * (state[1]).cos()
            + (dt * state[6]).cos() * (dt * state[7]).cos() * (state[0]).cos() * (state[1]).cos(),
    );
    out[1] = ((dt * state[7]).sin() * (state[0]).cos() * (state[1]).cos()
        - (dt * state[8]).sin() * (state[0]).sin() * (dt * state[7]).cos() * (state[1]).cos()
        + (state[1]).sin() * (dt * state[7]).cos() * (dt * state[8]).cos())
    .asin();
    out[2] = (-(-(state[0]).sin() * (state[2]).cos()
        + (state[1]).sin() * (state[2]).sin() * (state[0]).cos())
        * (dt * state[7]).sin()
        + ((state[0]).sin() * (state[1]).sin() * (state[2]).sin()
            + (state[0]).cos() * (state[2]).cos())
            * (dt * state[8]).sin()
            * (dt * state[7]).cos()
        + (state[2]).sin() * (dt * state[7]).cos() * (dt * state[8]).cos() * (state[1]).cos())
    .atan2(
        -((state[0]).sin() * (state[2]).sin()
            + (state[1]).sin() * (state[0]).cos() * (state[2]).cos())
            * (dt * state[7]).sin()
            + ((state[0]).sin() * (state[1]).sin() * (state[2]).cos()
                - (state[2]).sin() * (state[0]).cos())
                * (dt * state[8]).sin()
                * (dt * state[7]).cos()
            + (dt * state[7]).cos() * (dt * state[8]).cos() * (state[1]).cos() * (state[2]).cos(),
    );
    out[3] = dt * state[12] + state[3];
    out[4] = dt * state[13] + state[4];
    out[5] = dt * state[14] + state[5];
    out[6] = state[6];
    out[7] = state[7];
    out[8] = state[8];
    out[9] = state[9];
    out[10] = state[10];
    out[11] = state[11];
    out[12] = state[12];
    out[13] = state[13];
    out[14] = state[14];
    out[15] = state[15];
    out[16] = state[16];
    out[17] = state[17];
    out
}
