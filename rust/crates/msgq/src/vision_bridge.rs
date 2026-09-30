#[cxx::bridge(namespace = "openpilot_rust")]
pub(crate) mod ffi {
    #[derive(Debug, Clone, Copy)]
    struct ConnectionLayout {
        width: usize,
        height: usize,
        stride: usize,
        uv_offset: usize,
        len: usize,
        available: bool,
    }

    #[derive(Debug, Clone, Copy)]
    struct VisionMetadata {
        width: usize,
        height: usize,
        stride: usize,
        uv_offset: usize,
        len: usize,
        frame_id: u32,
        timestamp_sof: u64,
        timestamp_eof: u64,
        valid: bool,
        received: bool,
    }

    // SAFETY: C++ owns the original client and mapped buffers in UniquePtr.
    // It copies only into a checked exclusive destination, retains no Rust
    // borrow and returns metadata by value. No shared camera slice crosses FFI.
    unsafe extern "C++" {
        include!("vision.h");
        type VisionConnection;
        fn open_vision(
            name: &str,
            stream: i32,
            conflate: bool,
        ) -> Result<UniquePtr<VisionConnection>>;
        fn vision_streams(name: &str) -> Result<u32>;
        fn connect(self: Pin<&mut VisionConnection>) -> Result<bool>;
        fn connected(self: &VisionConnection) -> bool;
        fn layout(self: &VisionConnection) -> ConnectionLayout;
        fn receive(self: Pin<&mut VisionConnection>, timeout_ms: i32) -> Result<VisionMetadata>;
        fn copy_frame(self: &VisionConnection, destination: &mut [u8]) -> Result<()>;
    }
}
