// Generated from unchanged CarKalman symbolic equations by generate_car_model.py.
pub fn transition_jacobian(state: &[f64; 9], globals: &[f64; 6], dt: f64) -> [f64; 81] {
    let mass = globals[0];
    let rotational_inertia = globals[1];
    let center_to_front = globals[2];
    let center_to_rear = globals[3];
    let stiffness_front = globals[4];
    let stiffness_rear = globals[5];
    let mut out = [0.0; 81];
    out[0] = 1.0;
    out[10] = 1.0;
    out[20] = 1.0;
    out[30] = 1.0;
    out[40] = 1.0;
    out[45] = dt
        * (stiffness_front * (-state[2] - state[3] + state[7]) / (mass * state[1])
            + (-stiffness_front - stiffness_rear) * state[5] / (mass * state[4])
            + (-center_to_front * stiffness_front + center_to_rear * stiffness_rear) * state[6]
                / (mass * state[4]));
    out[46] = -dt * stiffness_front * (-state[2] - state[3] + state[7]) * state[0]
        / (mass * (state[1]).powi(2));
    out[47] = -dt * stiffness_front * state[0] / (mass * state[1]);
    out[48] = -dt * stiffness_front * state[0] / (mass * state[1]);
    out[49] = dt
        * ((-1.0
            - (-center_to_front * stiffness_front * state[0]
                + center_to_rear * stiffness_rear * state[0])
                / (mass * (state[4]).powi(2)))
            * state[6]
            - (-stiffness_front * state[0] - stiffness_rear * state[0]) * state[5]
                / (mass * (state[4]).powi(2)));
    out[50] =
        dt * (-stiffness_front * state[0] - stiffness_rear * state[0]) / (mass * state[4]) + 1.0;
    out[51] = dt
        * (-state[4]
            + (-center_to_front * stiffness_front * state[0]
                + center_to_rear * stiffness_rear * state[0])
                / (mass * state[4]));
    out[52] = dt * stiffness_front * state[0] / (mass * state[1]);
    out[53] = -9.81 * dt;
    out[54] = dt
        * (center_to_front * stiffness_front * (-state[2] - state[3] + state[7])
            / (rotational_inertia * state[1])
            + (-center_to_front * stiffness_front + center_to_rear * stiffness_rear) * state[5]
                / (rotational_inertia * state[4])
            + (-(center_to_front).powi(2) * stiffness_front
                - (center_to_rear).powi(2) * stiffness_rear)
                * state[6]
                / (rotational_inertia * state[4]));
    out[55] =
        -center_to_front * dt * stiffness_front * (-state[2] - state[3] + state[7]) * state[0]
            / (rotational_inertia * (state[1]).powi(2));
    out[56] = -center_to_front * dt * stiffness_front * state[0] / (rotational_inertia * state[1]);
    out[57] = -center_to_front * dt * stiffness_front * state[0] / (rotational_inertia * state[1]);
    out[58] = dt
        * (-(-center_to_front * stiffness_front * state[0]
            + center_to_rear * stiffness_rear * state[0])
            * state[5]
            / (rotational_inertia * (state[4]).powi(2))
            - (-(center_to_front).powi(2) * stiffness_front * state[0]
                - (center_to_rear).powi(2) * stiffness_rear * state[0])
                * state[6]
                / (rotational_inertia * (state[4]).powi(2)));
    out[59] = dt
        * (-center_to_front * stiffness_front * state[0]
            + center_to_rear * stiffness_rear * state[0])
        / (rotational_inertia * state[4]);
    out[60] = dt
        * (-(center_to_front).powi(2) * stiffness_front * state[0]
            - (center_to_rear).powi(2) * stiffness_rear * state[0])
        / (rotational_inertia * state[4])
        + 1.0;
    out[61] = center_to_front * dt * stiffness_front * state[0] / (rotational_inertia * state[1]);
    out[70] = 1.0;
    out[80] = 1.0;
    out
}
