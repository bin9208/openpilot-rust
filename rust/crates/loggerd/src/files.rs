use std::{fs::File, io, os::fd::IntoRawFd};

#[expect(
    unsafe_code,
    reason = "source ZstdFileWriter checks fclose errors without fsync"
)]
pub fn close(file: File) -> io::Result<()> {
    let descriptor = file.into_raw_fd();
    // SAFETY: into_raw_fd transfers exclusive ownership; try_close consumes the
    // live descriptor on Linux even on error, so it is never retried or reused.
    unsafe { rustix::io::try_close(descriptor) }.map_err(io::Error::from)
}
