//! NCM listener/DHCP lifetime retains the bare ep0 owner and never opens bulk.
use super::{
    platform::{self, Paths},
    root,
};
use crate::{ffs::Session, tcp::TcpTransport, wire::Version, Deadline, Error};
use std::{
    net::{Ipv4Addr, SocketAddr, TcpListener},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
const ADDRESS: Ipv4Addr = Ipv4Addr::new(192, 168, 60, 1);
fn interface(paths: &Paths) -> Result<(String, u32), Error> {
    let name = platform::text(&paths.function().join("ifname"))?;
    if !platform::valid_interface(&name) {
        return Err(Error::Contract("NCM has no usable interface name"));
    }
    let index = platform::text(&paths.net.join(&name).join("ifindex"))?
        .parse::<u32>()
        .map_err(|_| Error::Contract("invalid NCM interface index"))?;
    if index == 0 {
        return Err(Error::Contract("invalid NCM interface index"));
    }
    Ok((name, index))
}
pub fn cable_peer(peer: SocketAddr, local: SocketAddr) -> bool {
    match (peer, local) {
        (SocketAddr::V4(peer), SocketAddr::V4(local)) => {
            let p = peer.ip().octets();
            p[..3] == [192, 168, 60] && (2..=254).contains(&p[3]) && *local.ip() == ADDRESS
        }
        _ => false,
    }
}
pub struct MobileCable {
    owner: Arc<Session>,
    paths: Paths,
    identity: (String, u32),
    listener: Mutex<Option<TcpListener>>,
    network: AtomicBool,
    stopped: AtomicBool,
}
impl MobileCable {
    pub fn start(owner: Arc<Session>, timeout: Duration) -> Result<Arc<Self>, Error> {
        if timeout.is_zero() {
            return Err(Error::Contract("mobile start timeout"));
        }
        if !owner.bound()? {
            return Err(Error::Contract("bind ep0 before starting NCM listener"));
        }
        let paths = Paths::device();
        let until = Instant::now() + timeout;
        let identity = loop {
            match interface(&paths) {
                Ok(value) => break value,
                Err(error) => {
                    if Instant::now() >= until {
                        return Err(error);
                    }
                    std::thread::sleep(
                        Duration::from_millis(50)
                            .min(until.saturating_duration_since(Instant::now())),
                    );
                }
            }
        };
        let cable = Arc::new(Self {
            owner,
            paths,
            identity,
            listener: Mutex::new(None),
            network: AtomicBool::new(true),
            stopped: AtomicBool::new(false),
        });
        let result = (|| {
            root("net")?;
            cable.check_interface()?;
            let fd = rustix::net::socket_with(
                rustix::net::AddressFamily::INET,
                rustix::net::SocketType::STREAM,
                rustix::net::SocketFlags::CLOEXEC | rustix::net::SocketFlags::NONBLOCK,
                None,
            )
            .map_err(std::io::Error::from)?;
            rustix::net::sockopt::set_socket_reuseaddr(&fd, true).map_err(std::io::Error::from)?;
            rustix::net::bind(&fd, &rustix::net::SocketAddrV4::new(ADDRESS, 5599))
                .map_err(std::io::Error::from)?;
            rustix::net::listen(&fd, 1).map_err(std::io::Error::from)?;
            *cable
                .listener
                .lock()
                .map_err(|_| Error::Contract("NCM listener poisoned"))? =
                Some(TcpListener::from(fd));
            Ok(())
        })();
        if let Err(error) = result {
            if let Err(cleanup) = cable.close() {
                return Err(Error::Native(format!(
                    "{error}; unsafe mobile cleanup: {cleanup}"
                )));
            }
            return Err(error);
        }
        Ok(cable)
    }
    fn check_interface(&self) -> Result<(), Error> {
        if self.stopped.load(Ordering::Acquire) || !self.owner.bound()? {
            return Err(Error::Closed);
        }
        if interface(&self.paths)? != self.identity {
            return Err(Error::Contract(
                "NCM changed after rebind; restart cable listener",
            ));
        }
        Ok(())
    }
    pub fn accept(&self, deadline: Deadline) -> Result<TcpTransport, Error> {
        loop {
            deadline.remaining()?;
            self.check_interface()?;
            let accepted = {
                let listener = self
                    .listener
                    .lock()
                    .map_err(|_| Error::Contract("NCM listener poisoned"))?;
                listener.as_ref().ok_or(Error::Closed)?.accept()
            };
            match accepted {
                Ok((stream, peer)) => {
                    self.check_interface()?;
                    if !cable_peer(peer, stream.local_addr()?) {
                        drop(stream);
                        continue;
                    }
                    return TcpTransport::from_stream(stream, Version::V3);
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5).min(deadline.remaining()?))
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
    pub fn close(&self) -> Result<(), Error> {
        self.stopped.store(true, Ordering::Release);
        self.listener
            .lock()
            .map_err(|_| Error::Contract("NCM listener poisoned"))?
            .take();
        if self.network.load(Ordering::Acquire) {
            root("net-down")?;
            self.network.store(false, Ordering::Release);
        }
        Ok(())
    }
    pub fn interface_name(&self) -> &str {
        &self.identity.0
    }
}
impl Drop for MobileCable {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            eprintln!("unsafe mobile cable cleanup; restart Jetlink before retry: {error}");
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepted_peer_must_be_same_cable_subnet_and_exact_local_address() {
        let local = "192.168.60.1:5599".parse().unwrap();
        for address in 0..=255 {
            let peer = SocketAddr::from(([192, 168, 60, address], 1234));
            assert_eq!(cable_peer(peer, local), (2..=254).contains(&address));
        }
        for peer in ["192.168.61.2:5599", "127.0.0.1:5599", "[::1]:5599"] {
            assert!(!cable_peer(peer.parse().unwrap(), local));
        }
        assert!(!cable_peer(
            "192.168.60.2:1234".parse().unwrap(),
            "127.0.0.1:5599".parse().unwrap()
        ));
    }
}
