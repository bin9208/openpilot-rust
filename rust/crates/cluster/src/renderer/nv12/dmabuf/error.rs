#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Native(#[from] openpilot_startup_ui::Error),
    #[error("EGL has no current display")]
    Display,
    #[error("EGL_EXT_image_dma_buf_import is unavailable")]
    Extension,
    #[error("required EGLImage functions are unavailable")]
    Functions,
    #[error("encoder DMA-BUF has an invalid packed render layout")]
    Layout,
    #[error("failed to duplicate encoder DMA-BUF: {0}")]
    Duplicate(std::io::Error),
    #[error("EGL rejected encoder DMA-BUF with error 0x{0:04x}")]
    Import(i32),
    #[error("GLES error state did not clear before {0}")]
    Uncleared(&'static str),
    #[error("encoder DMA-BUF framebuffer failed status=0x{status:04x} error=0x{error:04x}")]
    Framebuffer { status: u32, error: u32 },
    #[error("GLES failed to create an encoder DMA-BUF fence")]
    FenceCreation,
    #[error("encoder DMA-BUF fence timed out")]
    Timeout,
    #[error("encoder DMA-BUF fence failed with GL error 0x{0:04x}")]
    Fence(u32),
    #[error("encoder DMA-BUF fence returned 0x{0:04x}")]
    FenceStatus(u32),
}
pub type Result<T> = std::result::Result<T, Error>;
