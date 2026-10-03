pub mod acquire;
pub mod bps;
mod bps_data;
pub mod bps_packet;
pub mod ife;
pub mod ife_packet;
mod program;

pub use program::Program;

#[derive(Debug, thiserror::Error)]
pub enum IspError {
    #[error(transparent)]
    Packing(#[from] crate::cdm::PackingError),
    #[error(transparent)]
    Packet(#[from] crate::packet::PacketError),
    #[error(transparent)]
    Layout(#[from] crate::nv12::LayoutError),
    #[error("camera command buffer offset exceeds its allocation")]
    Offset,
}

#[derive(Clone, Copy, Debug)]
pub struct BufferRef {
    pub handle: i32,
    pub size: u32,
    pub aligned_size: u32,
}

impl BufferRef {
    pub fn offset(self, slot: u32) -> Result<u32, IspError> {
        self.aligned_size.checked_mul(slot).ok_or(IspError::Offset)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FrameBuffers {
    pub raw: i32,
    pub yuv: i32,
    pub ife_fence: i32,
    pub bps_fence: i32,
}
