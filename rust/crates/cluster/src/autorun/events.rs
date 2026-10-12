use crate::uevent;
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    net::{bind, netlink, recv, socket_with, AddressFamily, RecvFlags, SocketFlags, SocketType},
};
use std::{os::fd::OwnedFd, time::Duration};

#[derive(Default)]
pub struct Events {
    socket: Option<OwnedFd>,
}

impl Events {
    #[must_use]
    pub fn open() -> Self {
        let mut last_error = None;
        for port in [0, std::process::id()] {
            let result = socket_with(
                AddressFamily::NETLINK,
                SocketType::DGRAM,
                SocketFlags::CLOEXEC | SocketFlags::NONBLOCK,
                Some(netlink::KOBJECT_UEVENT),
            )
            .and_then(|socket| {
                bind(&socket, &netlink::SocketAddrNetlink::new(port, 1))?;
                Ok(socket)
            });
            match result {
                Ok(socket) => return Self::from_socket(socket),
                Err(error) => last_error = Some(error),
            }
        }
        if let Some(error) = last_error {
            eprintln!("[cluster_autorun] USB event monitor unavailable: {error}");
        }
        Self::default()
    }

    /// Take ownership of a nonblocking datagram receiver, as the source socket argument requires.
    #[must_use]
    pub fn from_socket(socket: OwnedFd) -> Self {
        Self {
            socket: Some(socket),
        }
    }

    #[must_use]
    pub const fn available(&self) -> bool {
        self.socket.is_some()
    }

    #[must_use]
    pub fn wait(&self, timeout: Duration, expected_product: u16) -> bool {
        if timeout.is_zero() {
            return false;
        }
        let Some(socket) = self.socket.as_ref() else {
            std::thread::sleep(timeout);
            return false;
        };
        let mut descriptors = [PollFd::new(socket, PollFlags::IN)];
        let result = Timespec::try_from(timeout)
            .map_err(std::io::Error::other)
            .and_then(|timeout| poll(&mut descriptors, Some(&timeout)).map_err(Into::into));
        match result {
            Ok(0) => return false,
            Ok(_) => {}
            Err(error) => {
                eprintln!("[cluster_autorun] USB event wait failed: {error}");
                std::thread::sleep(timeout);
                return false;
            }
        }
        let mut matched = false;
        let mut payload = [0_u8; 8192];
        loop {
            match recv(socket, &mut payload[..], RecvFlags::empty()) {
                Ok((_, count)) => {
                    matched = uevent::matches(&payload[..count], expected_product) || matched;
                }
                Err(rustix::io::Errno::AGAIN) => return matched,
                Err(error) => {
                    eprintln!("[cluster_autorun] USB event read failed: {error}");
                    return matched;
                }
            }
        }
    }
}
