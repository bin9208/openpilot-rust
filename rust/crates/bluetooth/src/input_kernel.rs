use std::{collections::BTreeSet, fs::File, io, os::fd::AsRawFd};

/// Keys currently held, from EVIOCGKEY(96): Linux KEY_MAX=0x2ff incl. BTN_TOUCH.
pub(crate) fn pressed(file: &File) -> io::Result<BTreeSet<u16>> {
    let mut bitmap = [0_u8; 96];
    // SAFETY: EVIOCGKEY(96) writes at most 96 bytes into the live buffer.
    if unsafe { libc::ioctl(file.as_raw_fd(), 0x8060_4518, bitmap.as_mut_ptr()) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((0..768_u16)
        .filter(|key| bitmap[usize::from(key / 8)] & (1 << (key % 8)) != 0)
        .collect())
}

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
