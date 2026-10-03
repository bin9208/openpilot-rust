use crate::callbacks::{
    model_error, model_identity, model_observe, model_observe_jacobian, model_transition,
    model_transition_jacobian,
};

#[cxx::bridge(namespace = "locationd")]
pub mod ffi {
    pub struct Snapshot {
        pub time: f64,
        pub x: Vec<f64>,
        pub covariance: Vec<f64>,
    }
    pub struct Estimate {
        pub observed: bool,
        pub x: Vec<f64>,
        pub covariance: Vec<f64>,
        pub residual: Vec<f64>,
    }
    // SAFETY: unique ownership pins EKFSym; native entrypoints validate slice sizes,
    // copy inputs, retain no borrowed pointers, and callbacks borrow disjoint buffers synchronously.
    unsafe extern "C++" {
        include!("filter.h");
        type Filter;
        fn new_filter(x: &[f64], covariance: &[f64], noise: &[f64]) -> Result<UniquePtr<Filter>>;
        fn snapshot(self: &Filter) -> Snapshot;
        fn reset(self: Pin<&mut Filter>, x: &[f64], covariance: &[f64], time: f64) -> Result<()>;
        fn observe(
            self: Pin<&mut Filter>,
            time: f64,
            kind: i32,
            values: &[f64],
            noise: &[f64],
        ) -> Result<Estimate>;
        fn configure_scheduler(pc: bool) -> Result<()>;
    }
    extern "Rust" {
        fn model_transition(state: &[f64], dt: f64, out: &mut [f64]);
        fn model_transition_jacobian(state: &[f64], dt: f64, out: &mut [f64]);
        fn model_observe(kind: i32, state: &[f64], out: &mut [f64]);
        fn model_observe_jacobian(kind: i32, state: &[f64], out: &mut [f64]);
        fn model_error(state: &[f64], delta: &[f64], out: &mut [f64]);
        fn model_identity(out: &mut [f64]);
    }
}
