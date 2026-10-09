use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

pub struct Environment {
    pub local_ip: Arc<dyn Fn() -> String + Send + Sync>,
    pub time: Arc<dyn Fn() -> f64 + Send + Sync>,
}

pub fn select_ip(
    route: impl FnOnce() -> Option<String>,
    hostname: impl FnOnce() -> Option<String>,
) -> String {
    route()
        .or_else(hostname)
        .unwrap_or_else(|| "0.0.0.0".into())
}

fn route_ip(peer: SocketAddr) -> Option<String> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect(peer).ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}

pub fn hostname_ip(name: &str) -> Option<String> {
    (name, 0)
        .to_socket_addrs()
        .ok()?
        .find_map(|address| match address.ip() {
            IpAddr::V4(ip) => Some(ip.to_string()),
            IpAddr::V6(_) => None,
        })
}

pub fn owned_route_ip(peer: SocketAddr) -> Option<String> {
    peer.ip().is_loopback().then(|| route_ip(peer)).flatten()
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            local_ip: Arc::new(|| {
                select_ip(
                    || route_ip(SocketAddr::new(Ipv4Addr::new(8, 8, 8, 8).into(), 80)),
                    || {
                        nix::unistd::gethostname()
                            .ok()?
                            .to_str()
                            .and_then(hostname_ip)
                    },
                )
            }),
            time: Arc::new(|| match SystemTime::now().duration_since(UNIX_EPOCH) {
                Ok(value) => value.as_secs_f64(),
                Err(error) => -error.duration().as_secs_f64(),
            }),
        }
    }
}
