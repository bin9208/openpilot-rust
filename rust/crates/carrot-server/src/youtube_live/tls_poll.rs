use std::{io, os::fd::AsRawFd, time::Duration};

#[expect(
    unsafe_code,
    reason = "poll borrows live socket descriptors only for one bounded readiness wait"
)]
pub(super) fn readable(sockets: &[&impl AsRawFd], delay: Duration) -> io::Result<Vec<bool>> {
    let mut polls: Vec<_> = sockets
        .iter()
        .map(|socket| libc::pollfd {
            fd: socket.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        })
        .collect();
    let timeout = i32::try_from(delay.as_millis()).unwrap_or(i32::MAX);
    // SAFETY: polls is a live initialized contiguous array, its exact length is
    // passed, and each borrowed socket outlives the synchronous poll call.
    let result = unsafe {
        libc::poll(
            polls.as_mut_ptr(),
            libc::nfds_t::try_from(polls.len())
                .map_err(|_| io::Error::other("too many sockets"))?,
            timeout,
        )
    };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(polls
        .iter()
        .map(|poll| poll.revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0)
        .collect())
}
