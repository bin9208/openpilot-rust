use std::os::fd::BorrowedFd;

#[derive(Debug)]
pub struct VisionBufferDescriptor<'a> {
    pub fd: BorrowedFd<'a>,
    pub mmap_len: usize,
    pub data_len: usize,
    pub index: usize,
    pub server_id: u64,
    pub buffer_frame_id: u64,
}
