//! Compiler storage boundary from model_export/{buffers,offline_buffers}.py.
//! Range subtraction, alias/view identity, read/write order and raw weight
//! packing reuse the existing export core; no second allocator is introduced.
//! Register with the same `native-skip-miri` gate as `crate::export`.

pub use crate::export::{Allocation, Buffer, BufferTable, View};
use std::collections::BTreeMap;

/// Native lowering supplies a stable identity for each original *base object*,
/// not a content hash or an alias identity. Device spelling is preserved: the
/// original allocated-QCOM rejection compares the complete string to "QCOM".
#[derive(Clone, Copy, Debug)]
pub enum Descriptor<'a> {
    Single {
        buffer: Buffer,
        device: &'a str,
        allocated: bool,
    },
    /// A MultiBuffer or a value that is not a single Buffer.
    NonSingle,
}

pub fn host_buffer(value: Descriptor<'_>) -> Result<Buffer, String> {
    match value {
        Descriptor::Single { buffer, device, .. }
            if matches!(device.split(':').next(), Some("CPU" | "NPY" | "PYTHON")) =>
        {
            Ok(buffer)
        }
        _ => Err("CPU export requires single-device host buffers".into()),
    }
}

/// Equivalent to offline_buffers.descriptor after the caller resolves its UOp.
/// This inspects metadata only and never opens or allocates a runtime device.
pub fn descriptor(value: Descriptor<'_>) -> Result<Buffer, String> {
    match value {
        Descriptor::Single {
            buffer,
            device,
            allocated,
        } if matches!(
            device.split(':').next(),
            Some("QCOM" | "CPU" | "NPY" | "PYTHON")
        ) =>
        {
            if device == "QCOM" && allocated {
                return Err("offline export requires unallocated QCOM descriptors".into());
            }
            Ok(buffer)
        }
        _ => Err("single-device QCOM or host storage required".into()),
    }
}

/// Initializers are keyed by exact base-object identity. Lookup and full-base
/// byte-length validation are deferred until an unwritten range is read. A
/// missing/malformed initializer is harmless when prior writes cover the read.
/// Host storage follows the same initializer contract as QCOM in offline mode.
pub struct OfflineBufferTable<'a> {
    table: BufferTable,
    initialized: &'a BTreeMap<u64, Vec<u8>>,
}

impl<'a> OfflineBufferTable<'a> {
    pub fn new(initialized: &'a BTreeMap<u64, Vec<u8>>) -> Self {
        Self {
            table: BufferTable::default(),
            initialized,
        }
    }

    /// Call `descriptor` first; this layer retains the original table's storage
    /// API, independently of device admission at the compiler input boundary.
    pub fn view(&mut self, buffer: Buffer) -> Result<usize, String> {
        self.table.view(buffer)
    }

    pub fn read(&mut self, buffer: Buffer) -> Result<(), String> {
        self.table.read_initialized(
            buffer,
            self.initialized.get(&buffer.base).map(Vec::as_slice),
        )
    }

    pub fn write(&mut self, buffer: Buffer) -> Result<(), String> {
        self.table.write(buffer)
    }

    pub fn views(&self) -> &[View] {
        &self.table.views
    }

    pub fn weights(&self) -> (Vec<Allocation>, Vec<u8>) {
        self.table.weights()
    }
}
