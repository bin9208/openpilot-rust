#[cfg(not(all(
    target_endian = "little",
    target_pointer_width = "64",
    target_os = "linux"
)))]
compile_error!("openpilot-msgq requires the original 64-bit little-endian Linux ABI");

mod error;
pub use error::Error;
#[cfg(feature = "native-skip-miri")]
mod mapped;
#[cfg(any(feature = "native-skip-miri", test))]
mod memory;
#[cfg(feature = "native-skip-miri")]
mod queue;
#[cfg(any(feature = "native-skip-miri", test))]
mod queue_core;
#[cfg(feature = "native-skip-miri")]
mod transport;
#[cfg(any(feature = "native-skip-miri", test))]
mod vision_types;
#[cfg(any(feature = "native-skip-miri", test))]
mod vision_wire;
#[cfg(feature = "native-skip-miri")]
pub use transport::{MultiSubscriber, Publisher, QueuedMessage, Subscriber, Subscription};
#[cfg(feature = "visionipc-ion")]
mod ion;
#[cfg(feature = "native-skip-miri")]
mod vision;
#[cfg(feature = "native-skip-miri")]
mod vision_buffer;
#[cfg(feature = "native-skip-miri")]
mod vision_memory;
#[cfg(feature = "native-skip-miri")]
mod vision_socket;
#[cfg(feature = "native-skip-miri")]
pub use vision::{VisionClient, VisionFrame};
#[cfg(any(feature = "native-skip-miri", test))]
pub use vision_types::{VisionLayout, VisionMetadata, VisionStream};
#[cfg(feature = "native-skip-miri")]
mod vision_server;
#[cfg(feature = "native-skip-miri")]
pub use vision_buffer::VisionBufferDescriptor;
#[cfg(feature = "native-skip-miri")]
pub use vision_server::{RawVisionImage, VisionImage, VisionServer};

#[cfg(all(test, feature = "native-skip-miri"))]
mod abi_tests;

#[cfg(all(test, feature = "native-skip-miri"))]
#[path = "../tests/support/mod.rs"]
mod test_process;

#[cfg(all(test, feature = "native-skip-miri", not(feature = "visionipc-ion")))]
mod vision_boundary_tests;
