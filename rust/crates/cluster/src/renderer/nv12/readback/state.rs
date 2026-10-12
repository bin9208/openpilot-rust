pub(super) const GL_FRAMEBUFFER: u32 = 0x8D40;
pub(super) const GL_FRAMEBUFFER_BINDING: u32 = 0x8CA6;
pub(super) const GL_PACK_ALIGNMENT: u32 = 0x0D05;
pub(super) const GL_PIXEL_PACK_BUFFER: u32 = 0x88EB;
pub(super) const GL_PIXEL_PACK_BUFFER_BINDING: u32 = 0x88ED;
pub(super) const GL_STREAM_READ: u32 = 0x88E1;
pub(super) const GL_SYNC_GPU_COMMANDS_COMPLETE: u32 = 0x9117;
pub(super) const GL_ALREADY_SIGNALED: u32 = 0x911A;
pub(super) const GL_TIMEOUT_EXPIRED: u32 = 0x911B;
pub(super) const GL_CONDITION_SATISFIED: u32 = 0x911C;
pub(super) const GL_WAIT_FAILED: u32 = 0x911D;
pub(super) struct Slot {
    pub buffer: u32,
    pub fence: Option<u64>,
    pub bytes: usize,
    pub submitted: f64,
}
pub(super) fn binding(value: i32) -> u32 {
    u32::from_ne_bytes(value.to_ne_bytes())
}
