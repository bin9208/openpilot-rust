//! Owned POSIX PTY descriptors, shared by independent terminal owners.
//!
//! The original web terminal uses openpty and a new session without TIOCSCTTY.
//! Allocation and geometry stay independent of HTTP and codec runtimes.
use std::fs::File;

pub struct Pair {
    pub master: File,
    pub slave: File,
}
impl Pair {
    pub fn open(rows: u16, cols: u16) -> std::io::Result<Self> {
        use rustix::pty::{grantpt, ioctl_tiocgptpeer, openpt, ptsname, unlockpt, OpenptFlags};
        let flags = OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC;
        let master = openpt(flags)?;
        grantpt(&master)?;
        unlockpt(&master)?;
        let slave = match ioctl_tiocgptpeer(&master, flags) {
            Ok(slave) => slave,
            Err(rustix::io::Errno::NOTTY | rustix::io::Errno::INVAL | rustix::io::Errno::NOSYS) => {
                let path = ptsname(&master, Vec::new())?;
                rustix::fs::open(&path, flags.into(), rustix::fs::Mode::empty())?
            }
            Err(error) => return Err(error.into()),
        };
        let pair = Self {
            master: master.into(),
            slave: slave.into(),
        };
        resize(&pair.master, rows, cols)?;
        Ok(pair)
    }
}

pub fn resize(fd: impl std::os::fd::AsFd, rows: u16, cols: u16) -> std::io::Result<()> {
    rustix::termios::tcsetwinsize(
        fd,
        rustix::termios::Winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        },
    )?;
    Ok(())
}
