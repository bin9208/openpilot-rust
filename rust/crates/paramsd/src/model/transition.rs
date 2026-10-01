// Generated from unchanged CarKalman symbolic equations by generate_car_model.py.
pub fn transition(state: &[f64; 9], globals: &[f64; 6], dt: f64) -> [f64; 9] {
    let mass = globals[0];
    let rotational_inertia = globals[1];
    let center_to_front = globals[2];
    let center_to_rear = globals[3];
    let stiffness_front = globals[4];
    let stiffness_rear = globals[5];
    let mut out = [0.0; 9];
    out[0] = state[0];
    out[1] = state[1];
    out[2] = state[2];
    out[3] = state[3];
    out[4] = state[4];
    out[5] = dt
        * ((-state[4]
            + (-center_to_front * stiffness_front * state[0]
                + center_to_rear * stiffness_rear * state[0])
                / (mass * state[4]))
            * state[6]
            - 9.81 * state[8]
            + stiffness_front * (-state[2] - state[3] + state[7]) * state[0] / (mass * state[1])
            + (-stiffness_front * state[0] - stiffness_rear * state[0]) * state[5]
                / (mass * state[4]))
        + state[5];
    out[6] = dt
        * (center_to_front * stiffness_front * (-state[2] - state[3] + state[7]) * state[0]
            / (rotational_inertia * state[1])
            + (-center_to_front * stiffness_front * state[0]
                + center_to_rear * stiffness_rear * state[0])
                * state[5]
                / (rotational_inertia * state[4])
            + (-(center_to_front).powi(2) * stiffness_front * state[0]
                - (center_to_rear).powi(2) * stiffness_rear * state[0])
                * state[6]
                / (rotational_inertia * state[4]))
        + state[6];
    out[7] = state[7];
    out[8] = state[8];
    out
}
