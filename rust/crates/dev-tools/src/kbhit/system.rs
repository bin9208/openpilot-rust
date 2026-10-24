//! Small Linux libc bridge via the already-existing optional nix dependency.
use nix::libc;
use std::{
    io,
    os::fd::{AsRawFd, BorrowedFd},
    ptr,
};

pub(super) fn register_exit(callback: extern "C" fn()) -> io::Result<()> {
    // SAFETY: callback has C ABI, static lifetime, and catches Rust panics internally.
    if unsafe { libc::atexit(callback) } == 0 {
        Ok(())
    } else {
        Err(io::Error::other(
            "KBHit atexit registration failed after terminal update",
        ))
    }
}
pub(super) fn check_fd(fd: i32) -> io::Result<()> {
    if fd < 0 || fd as usize >= libc::FD_SETSIZE {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "filedescriptor out of range in select",
        ))
    } else {
        Ok(())
    }
}
pub(super) fn readable(fd: BorrowedFd<'_>) -> io::Result<bool> {
    let raw = fd.as_raw_fd();
    check_fd(raw)?;
    loop {
        // SAFETY: fd_set accepts all-zero bits; range check precedes FD_SET.
        let mut readfds: libc::fd_set = unsafe { std::mem::zeroed() };
        let mut timeout = libc::timeval {
            tv_sec: 0,
            tv_usec: 0,
        };
        // SAFETY: live BorrowedFd, bounded fd_set, valid zero-timeout pointer.
        let result = unsafe {
            libc::FD_ZERO(&mut readfds);
            libc::FD_SET(raw, &mut readfds);
            libc::select(
                raw + 1,
                &mut readfds,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut timeout,
            )
        };
        if result >= 0 {
            return Ok(result != 0);
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

#[cfg(test)]
pub(super) fn private_pty() -> io::Result<(std::fs::File, std::os::fd::OwnedFd)> {
    use std::os::fd::{FromRawFd, OwnedFd};
    // SAFETY: newly allocated private PTY; O_NOCTTY never acquires a controlling tty.
    let raw = unsafe { libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC) };
    if raw < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful allocation transfers its one new descriptor to OwnedFd.
    let master = unsafe { OwnedFd::from_raw_fd(raw) };
    // SAFETY: all calls operate exclusively on this owned private master.
    if unsafe { libc::grantpt(raw) } != 0 || unsafe { libc::unlockpt(raw) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let mut name = [0 as libc::c_char; 4096];
    // SAFETY: valid output buffer and length; ptsname_r returns an errno value.
    let error = unsafe { libc::ptsname_r(raw, name.as_mut_ptr(), name.len()) };
    if error != 0 {
        return Err(io::Error::from_raw_os_error(error));
    }
    // SAFETY: successful ptsname_r provides a NUL-terminated private slave path;
    // no O_CREAT vararg is needed, and O_NOCTTY avoids controlling-terminal effects.
    let slave = unsafe {
        libc::open(
            name.as_ptr(),
            libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC,
        )
    };
    if slave < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the checked open returned a distinct owned descriptor.
    Ok((std::fs::File::from(master), unsafe {
        OwnedFd::from_raw_fd(slave)
    }))
}
#[cfg(test)]
pub(super) fn wait_readable(fd: BorrowedFd<'_>) -> io::Result<()> {
    let mut item = libc::pollfd {
        fd: fd.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: one live supplied private PTY descriptor; finite test-only1000ms bound.
    let result = unsafe { libc::poll(&mut item, 1, 1000) };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    if result == 1 && item.revents & libc::POLLIN != 0 {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "private PTY input readiness missing",
        ))
    }
}
