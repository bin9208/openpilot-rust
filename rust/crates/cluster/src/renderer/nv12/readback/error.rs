#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Native(#[from] openpilot_startup_ui::Error),
    #[error("direct NV12 readback received an invalid render target")]
    InvalidTarget,
    #[error("asynchronous NV12 readback received an invalid render target")]
    InvalidAsyncTarget,
    #[error("{} NV12 {}destination is {actual} bytes, expected at least {required}", if *asynchronous {"asynchronous"} else {"direct"}, if *asynchronous {""} else {"readback "})]
    Destination {
        actual: usize,
        required: usize,
        asynchronous: bool,
    },
    #[error("GLES error state did not clear before {0}")]
    Uncleared(&'static str),
    #[error("GLES {operation} failed with GL error 0x{code:04x}")]
    Gl { operation: &'static str, code: u32 },
    #[error("asynchronous GLES NV12 readback is not available")]
    AsyncUnavailable,
    #[error("GLES returned an invalid pixel-pack buffer")]
    InvalidBuffer,
    #[error("GLES failed to create an NV12 readback fence")]
    FenceCreation,
    #[error("asynchronous NV12 readback lost its fence")]
    LostFence,
    #[error("asynchronous NV12 readback fence stalled")]
    Stalled,
    #[error("GLES NV12 fence returned unexpected status 0x{0:04x}")]
    FenceStatus(u32),
    #[error("GLES failed to map a completed NV12 readback")]
    Mapping,
    #[error("GLES reported a corrupted NV12 PBO mapping")]
    CorruptedMapping,
}
pub type Result<T> = std::result::Result<T, Error>;
