use std::{
    net::{Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket},
    sync::mpsc,
    thread::{self, JoinHandle},
    time::Duration,
};

pub(super) fn detect(host: &str) -> String {
    if !matches!(host, "" | "0.0.0.0" | "::") {
        return host.into();
    }
    if let Ok(socket) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)) {
        if socket.connect((Ipv4Addr::new(8, 8, 8, 8), 80)).is_ok() {
            if let Ok(address) = socket.local_addr() {
                return address.ip().to_string();
            }
        }
    }
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .and_then(|name| (name.trim(), 80).to_socket_addrs().ok())
        .and_then(|mut addresses| addresses.find(std::net::SocketAddr::is_ipv4))
        .map_or_else(|| "127.0.0.1".into(), |address| address.ip().to_string())
}

pub(super) struct Beacon {
    stop: Option<mpsc::Sender<()>>,
    worker: Option<JoinHandle<()>>,
}
impl Beacon {
    pub fn start(advertised: &str) -> Result<Self, std::io::Error> {
        Self::start_at(
            advertised,
            (Ipv4Addr::BROADCAST, 7705).into(),
            Duration::from_secs(1),
        )
    }
    fn start_at(
        advertised: &str,
        target: SocketAddr,
        interval: Duration,
    ) -> Result<Self, std::io::Error> {
        let body = openpilot_carrot_navi::json::Value::object([
            ("ip", openpilot_carrot_navi::json::Value::text(advertised)),
            ("navi_debug", openpilot_carrot_navi::json::Value::integer(1)),
        ])
        .encode()
        .map_err(std::io::Error::other)?
        .into_bytes();
        let (stop, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("cluster-navi-discovery".into())
            .spawn(move || loop {
                if receiver.try_recv() != Err(mpsc::TryRecvError::Empty) {
                    break;
                }
                if let Ok(socket) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)) {
                    if socket.set_broadcast(true).is_ok() {
                        let _ = socket.send_to(&body, target);
                    }
                }
                match receiver.recv_timeout(interval.max(Duration::from_millis(200))) {
                    Err(mpsc::RecvTimeoutError::Timeout) => (),
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            })?;
        Ok(Self {
            stop: Some(stop),
            worker: Some(worker),
        })
    }
    pub fn close(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for Beacon {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_udp_peer_receives_source_packet_and_close_interrupts_the_interval() {
        let peer = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut beacon = Beacon::start_at(
            "127.0.0.1",
            peer.local_addr().unwrap(),
            Duration::from_secs(1),
        )
        .unwrap();
        let mut bytes = [0; 256];
        let length = peer.recv(&mut bytes).unwrap();
        assert_eq!(
            &bytes[..length],
            b"{\"ip\": \"127.0.0.1\", \"navi_debug\": 1}"
        );
        let start = std::time::Instant::now();
        beacon.close();
        assert!(start.elapsed() < Duration::from_millis(250));
        beacon.close();
        peer.set_read_timeout(Some(Duration::from_millis(250)))
            .unwrap();
        assert!(peer.recv(&mut bytes).is_err());
    }
}
