use crate::{vision_bridge::ffi, Error};
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

#[allow(unsafe_code)]
pub(crate) fn descriptor(
    connection: &ffi::VisionConnection,
) -> Result<VisionBufferDescriptor<'_>, Error> {
    let value = connection.frame_descriptor()?;
    // SAFETY: the native FD is validated and remains owned by the borrowed
    // connection; the frame borrow excludes reconnect and client destruction.
    let fd = unsafe { BorrowedFd::borrow_raw(value.fd) };
    Ok(VisionBufferDescriptor {
        fd,
        mmap_len: value.mmap_len,
        data_len: value.data_len,
        index: value.index,
        server_id: value.server_id,
        buffer_frame_id: value.buffer_frame_id,
    })
}
