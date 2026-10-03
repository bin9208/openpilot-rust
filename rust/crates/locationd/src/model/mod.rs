mod transition;
pub use transition::transition;
mod transition_jacobian;
pub use transition_jacobian::transition_jacobian;
mod observe_4;
pub use observe_4::observe_4;
mod jacobian_4;
pub use jacobian_4::jacobian_4;
mod observe_10;
pub use observe_10::observe_10;
mod jacobian_10;
pub use jacobian_10::jacobian_10;
mod observe_13;
pub use observe_13::observe_13;
mod jacobian_13;
pub use jacobian_13::jacobian_13;
mod observe_14;
pub use observe_14::observe_14;
mod jacobian_14;
pub use jacobian_14::jacobian_14;
pub const INITIAL_X: [f64; 18] = [
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
];
pub const INITIAL_P: [f64; 18] = [
    0.0001, 0.0001, 0.0001, 100.0, 100.0, 100.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 10000.0, 10000.0,
    10000.0, 0.0001, 0.0001, 0.0001,
];
pub const PROCESS_NOISE: [f64; 18] = [
    1e-06,
    1e-06,
    1e-06,
    0.0001,
    0.0001,
    0.0001,
    0.007225000000000001,
    0.007225000000000001,
    0.007225000000000001,
    2.5e-09,
    2.5e-09,
    2.5e-09,
    9.0,
    9.0,
    9.0,
    2.5e-05,
    2.5e-05,
    2.5e-05,
];
pub const NOISE_4: [f64; 3] = [
    0.0006250000000000001,
    0.0006250000000000001,
    0.0006250000000000001,
];
pub const NOISE_10: [f64; 3] = [0.5625, 0.5625, 0.5625];
pub const NOISE_13: [f64; 3] = [0.25, 0.25, 0.25];
pub const NOISE_14: [f64; 3] = [
    0.0025000000000000005,
    0.0025000000000000005,
    0.0025000000000000005,
];
