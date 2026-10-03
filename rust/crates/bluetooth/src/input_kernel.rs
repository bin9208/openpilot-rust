use std::{fs::File, io, os::fd::AsRawFd};

pub(crate) fn claim(file: &File) -> io::Result<()> {
    // SAFETY: Linux EVIOCGRAB consumes an integer flag, not a pointer; File keeps the fd live.
    if unsafe { libc::ioctl(file.as_raw_fd(), 0x4004_4590, 1_i32) } < 0 {
        return Err(io::Error::last_os_error());
    }
    let clock: libc::c_int = libc::CLOCK_MONOTONIC;
    // SAFETY: EVIOCSCLOCKID reads one aligned c_int during this call; clock and the fd stay live.
    if unsafe { libc::ioctl(file.as_raw_fd(), 0x4004_45a0, &raw const clock) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
