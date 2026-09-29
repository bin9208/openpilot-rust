mod arguments;
mod descriptor;
mod device;
mod graph;
mod packet;
pub use graph::QcomGraph;
#[cfg(feature = "native-skip-miri")]
mod bundle;
#[cfg(feature = "native-skip-miri")]
pub use bundle::QcomBundle;
#[cfg(all(
    feature = "native-skip-miri",
    target_os = "linux",
    target_pointer_width = "64"
))]
mod model;
#[cfg(all(
    feature = "native-skip-miri",
    target_os = "linux",
    target_pointer_width = "64"
))]
pub use model::QcomModel;

use crate::Error;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ProgramImage {
    image_size: u32,
    prg_offset: u32,
    brnchstck: u32,
    pvtmem: u32,
    shmem: u32,
    samp_cnt: u32,
    samplers: Vec<u32>,
    buf_offs: Vec<u32>,
    tex_cnt: u32,
    ibo_cnt: u32,
    ibo_off: u32,
    tex_off: u32,
    samp_off: u32,
    consts_info: Vec<(u32, u32, u32)>,
    fregs: u32,
    hregs: u32,
    pvtmem_size_per_item: u32,
    pvtmem_size_total: u32,
    hw_stack_offset: u32,
    shared_size: u32,
    max_threads: u32,
    kernargs_alloc_size: u32,
    #[serde(skip)]
    image: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
pub enum Argument {
    Buffer {
        address: u64,
    },
    Image {
        address: u64,
        width: u32,
        height: u32,
        pitch: u32,
        element_bytes: u32,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct Dispatch {
    pub program: u64,
    pub stack: u64,
    pub border: u64,
    pub dummy: u64,
    pub args: u64,
    pub global: [f64; 3],
    pub local: [u32; 3],
}

fn invalid() -> Error {
    Error::Contract("invalid QCOMCL program")
}
