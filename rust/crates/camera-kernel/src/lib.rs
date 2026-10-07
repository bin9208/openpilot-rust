#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(all(
    feature = "native-skip-miri",
    any(
        not(target_os = "linux"),
        not(target_pointer_width = "64"),
        not(target_endian = "little")
    )
))]
compile_error!("camera kernel ABI requires 64-bit little-endian Linux");

#[cfg(feature = "native-skip-miri")]
mod native;
#[cfg(feature = "native-skip-miri")]
pub use native::{parse_double_prefix, random_unit, DoubleParseError};
#[cfg(feature = "native-skip-miri")]
pub use native::{set_diagnostic_handler, KernelDiagnostic};
#[cfg(feature = "native-skip-miri")]
pub use native::{Allocation, AllocationOptions, MemoryPool, PacketLease};
#[cfg(feature = "native-skip-miri")]
pub use native::{
    CallResult, CreatedFences, Device, DeviceHandle, DeviceOperation, Error, Fence, Link, Master,
    MmuHandles, Session,
};
#[cfg(feature = "native-skip-miri")]
pub use native::{CameraEvent, ImportedBuffers, PollResult};
