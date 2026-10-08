mod control;
mod devices;
mod diagnostics;
mod discovery;
mod events;
mod import;
mod mapping;
mod master;
mod memory;
mod requests;
mod stress;
mod sync;

pub use control::{CallResult, Device};
pub use devices::{DeviceHandle, DeviceOperation, Session};
pub use diagnostics::{set_diagnostic_handler, KernelDiagnostic};
pub use events::{CameraEvent, PollResult};
pub use import::ImportedBuffers;
pub use master::{Master, MmuHandles};
pub use memory::{Allocation, AllocationOptions, MemoryPool, PacketLease};
pub use requests::Link;
pub use stress::{parse_double_prefix, random_unit, DoubleParseError};
pub use sync::{CreatedFences, Fence};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("camera device path contains a NUL byte")]
    Path,
    #[error("opening camera device {path}: {source}")]
    Open {
        path: String,
        source: std::io::Error,
    },
    #[error("camera subdevice {name} index {index} was not found")]
    MissingDevice { name: String, index: usize },
    #[error("camera ioctl {opcode:#x} returned {code} (errno {errno})")]
    Control { opcode: u32, code: i32, errno: i32 },
    #[error("camera allocation length or alignment is outside the supported ABI")]
    AllocationSize,
    #[error("camera allocation returned unusable fd {fd} and handle {handle:#x}")]
    Allocation { fd: i32, handle: u32 },
    #[error("mapping camera allocation: {0}")]
    Mapping(std::io::Error),
    #[error(
        "camera memory write at {offset} with length {length} exceeds allocation size {capacity}"
    )]
    MemoryBounds {
        offset: usize,
        length: usize,
        capacity: usize,
    },
}

#[repr(C, align(8))]
struct Buffer<const N: usize>([u8; N]);

impl<const N: usize> Buffer<N> {
    fn zeroed() -> Self {
        Self([0; N])
    }
    fn put32(&mut self, offset: usize, value: u32) {
        self.0[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn put64(&mut self, offset: usize, value: u64) {
        self.0[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
    fn get32(&self, offset: usize) -> u32 {
        u32::from_le_bytes([
            self.0[offset],
            self.0[offset + 1],
            self.0[offset + 2],
            self.0[offset + 3],
        ])
    }
    fn get64(&self, offset: usize) -> u64 {
        let mut bytes = [0; 8];
        bytes.copy_from_slice(&self.0[offset..offset + 8]);
        u64::from_le_bytes(bytes)
    }
    fn address(&mut self) -> u64 {
        self.0.as_mut_ptr().expose_provenance() as u64
    }
}
