pub mod bus_lock;
pub mod check;
pub mod client;
pub mod clock;
pub mod hardware;
#[cfg(feature = "native-skip-miri")]
pub mod native_runtime;
#[cfg(feature = "native-skip-miri")]
pub mod native_usb;
pub mod transport;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[cfg(feature = "native-skip-miri")]
    #[error(transparent)]
    ModelRuntime(#[from] openpilot_model_runtime::Error),
    #[cfg(feature = "native-skip-miri")]
    #[error(transparent)]
    Library(#[from] libloading::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("{operation}: {message}")]
    UsbApi {
        operation: &'static str,
        code: i32,
        message: String,
    },
    #[error("{0}")]
    Protocol(String),
    #[error("{0}")]
    Contract(&'static str),
    #[error("short eGPU power status ({0} bytes)")]
    ShortPower(usize),
    #[error("Cannot allocate {0} bytes")]
    Allocation(u64),
    #[error("{operation}: timed out after {milliseconds} ms ({last:?} != {expected})")]
    Timeout {
        operation: String,
        milliseconds: u64,
        last: Option<u64>,
        expected: u64,
    },
    #[error("GPU check cancelled")]
    Cancelled,
}

pub mod usb3;

pub mod custom_asm;

pub mod model;
#[cfg(feature = "native-skip-miri")]
pub mod qcom_warp;
pub mod warp_validation;

pub mod stock_asm;

pub mod controller;
pub mod pci;

pub mod allocator;

pub mod device;

pub mod page_table;

pub mod memory;

pub mod amd_metadata;

pub mod discovery;

pub mod firmware;

pub mod amd_bus;
pub mod asic;
pub mod asic_gfx;
mod asic_gmc;
mod asic_ih;
mod asic_psp;
pub mod asic_sdma;
mod asic_smu;

pub mod packets;

pub mod queue;

pub mod elf;

pub mod kernel;

mod kernel_graph;
pub mod probe;
pub mod runtime_bus;
pub mod warp;
pub mod worker;
pub mod worker_native;

pub mod gpu_memory;

pub mod gpu;

pub mod hcq_gpu;
pub mod hcq_model;
pub mod hcq_vm;
