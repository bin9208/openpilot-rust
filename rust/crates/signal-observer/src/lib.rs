//! Native camera signal evidence. Control decisions remain in the planner.
#[cfg(feature = "native-skip-miri")]
pub mod detector;
#[cfg(feature = "native-skip-miri")]
pub mod pixels;
#[cfg(feature = "native-skip-miri")]
pub mod process;
pub mod transport;

#[cfg(feature = "native-skip-miri")]
pub mod worker;

pub mod scheduling;

pub mod backend;

#[cfg(feature = "daemon")]
pub mod runtime;

#[cfg(feature = "native-skip-miri")]
pub mod neural_sequence;

pub mod numpy_sort;

#[cfg(feature = "native-skip-miri")]
pub mod neural;

pub mod numpy_exp;

pub mod lifecycle;

#[cfg(all(feature = "daemon", any(feature = "neural-ort", feature = "neural-qcom")))]
pub mod neural_runtime;

pub mod auto;

#[cfg(feature = "daemon")]
pub mod auto_process;

#[cfg(feature = "daemon")]
pub mod auto_runtime;

#[cfg(feature = "daemon")]
pub mod entry;

#[cfg(feature = "native-skip-miri")]
pub mod qcom;

#[cfg(feature = "neural-qcom")]
pub mod qcom_provider;
