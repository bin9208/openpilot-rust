use crate::Error;
use nix::sys::socket::{self, sockopt, AddressFamily, SockFlag, SockType, SockaddrIn};
use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
    os::fd::AsRawFd,
    sync::Arc,
};

pub(super) struct Sockets {
    pub tx: Arc<tokio::net::UdpSocket>,
    pub rx: Arc<tokio::net::UdpSocket>,
    pub recipient: SocketAddr,
}

fn bound(address: SocketAddrV4) -> Result<UdpSocket, Error> {
    let socket = socket::socket(
        AddressFamily::Inet,
        SockType::Datagram,
        SockFlag::SOCK_CLOEXEC | SockFlag::SOCK_NONBLOCK,
        None,
    )?;
    socket::setsockopt(&socket, sockopt::ReuseAddr, &true)?;
    socket::setsockopt(&socket, sockopt::ReusePort, &true)?;
    socket::bind(socket.as_raw_fd(), &SockaddrIn::from(address))?;
    Ok(UdpSocket::from(socket))
}

impl Sockets {
    pub fn new(owned_recipient: Option<SocketAddr>) -> Result<Self, Error> {
        if let Some(recipient) = owned_recipient {
            let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
            socket.set_nonblocking(true)?;
            let socket = Arc::new(tokio::net::UdpSocket::from_std(socket)?);
            return Ok(Self {
                tx: Arc::clone(&socket),
                rx: socket,
                recipient,
            });
        }
        let multicast = Ipv4Addr::new(224, 0, 0, 251);
        let tx = bound(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 5353))?;
        let rx = bound(SocketAddrV4::new(multicast, 5353))?;
        rx.join_multicast_v4(&multicast, &Ipv4Addr::UNSPECIFIED)?;
        Ok(Self {
            tx: Arc::new(tokio::net::UdpSocket::from_std(tx)?),
            rx: Arc::new(tokio::net::UdpSocket::from_std(rx)?),
            recipient: SocketAddr::V4(SocketAddrV4::new(multicast, 5353)),
        })
    }
}
