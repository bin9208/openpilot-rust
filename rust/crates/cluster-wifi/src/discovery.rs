use crate::{mac, Error, Registration, DISCOVER, FOUND, PORT};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::{
    io,
    net::UdpSocket,
    time::{Duration, Instant},
};

pub trait Datagram {
    fn broadcast(&mut self) -> io::Result<()>;
    fn send_to(&mut self, request: &[u8], target: &str, port: u16) -> io::Result<()>;
    fn timeout(&mut self, seconds: f64) -> io::Result<()>;
    fn receive(&mut self, maximum: usize) -> io::Result<(Vec<u8>, String)>;
}
/// Deterministic IO/clock boundary for source comparisons and loopback tests.
/// The normal sender always passes PORT; no alternate port setting is exposed.
pub fn discover_with<D: Datagram>(
    registration: &Registration,
    timeout: f64,
    targets: &[&str],
    port: u16,
    nonce: impl FnOnce() -> Result<[u8; 16], Error>,
    mut clock: impl FnMut() -> f64,
    create: impl FnOnce() -> io::Result<D>,
) -> Result<String, Error> {
    let nonce = nonce()?;
    let mut request = DISCOVER.to_vec();
    request.extend(registration.device);
    request.extend(nonce);
    request.extend(mac(&registration.key, &request));
    let deadline = clock() + timeout;
    let mut socket = create()?;
    socket.broadcast()?;
    for target in targets {
        socket.send_to(&request, target, port)?;
    }
    while clock() < deadline {
        socket.timeout(0.01_f64.max(deadline - clock()))?;
        let (reply, address) = match socket.receive(128) {
            Ok(value) => value,
            Err(error) if error.kind() == io::ErrorKind::TimedOut => break,
            Err(error) => return Err(error.into()),
        };
        let mut prefix = FOUND.to_vec();
        prefix.extend(registration.device);
        prefix.extend(nonce);
        if reply.len() == 58 && reply[..26] == prefix {
            let mut mac = Hmac::<Sha256>::new_from_slice(&registration.key)
                .expect("HMAC accepts a 32-byte key");
            mac.update(&reply[..26]);
            if mac.verify_slice(&reply[26..]).is_ok() {
                return Ok(address);
            }
        }
    }
    Err(Error::NotFound)
}
struct Udp(UdpSocket);
impl Udp {
    fn new() -> io::Result<Self> {
        use rustix::net::{ipproto, socket_with, AddressFamily, SocketFlags, SocketType};
        let fd = socket_with(
            AddressFamily::INET,
            SocketType::DGRAM,
            SocketFlags::CLOEXEC,
            Some(ipproto::UDP),
        )?;
        Ok(Self(UdpSocket::from(fd)))
    }
}
impl Datagram for Udp {
    fn broadcast(&mut self) -> io::Result<()> {
        self.0.set_broadcast(true)
    }
    fn send_to(&mut self, request: &[u8], target: &str, port: u16) -> io::Result<()> {
        self.0.send_to(request, (target, port)).map(|_| ())
    }
    fn timeout(&mut self, seconds: f64) -> io::Result<()> {
        let duration = Duration::try_from_secs_f64(seconds).map_err(io::Error::other)?;
        self.0.set_read_timeout(Some(duration))
    }
    fn receive(&mut self, maximum: usize) -> io::Result<(Vec<u8>, String)> {
        let mut data = vec![0; maximum];
        match self.0.recv_from(&mut data) {
            Ok((size, address)) => {
                data.truncate(size);
                Ok((data, address.ip().to_string()))
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                Err(io::Error::new(io::ErrorKind::TimedOut, "timed out"))
            }
            Err(error) => Err(error),
        }
    }
}
pub fn discover(
    registration: &Registration,
    timeout: Duration,
    targets: &[&str],
) -> Result<String, Error> {
    let start = Instant::now();
    discover_with(
        registration,
        timeout.as_secs_f64(),
        targets,
        PORT,
        || {
            let mut nonce = [0; 16];
            getrandom::fill(&mut nonce).map_err(|error| io::Error::other(error.to_string()))?;
            Ok(nonce)
        },
        || start.elapsed().as_secs_f64(),
        Udp::new,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_udp_loopback_rejects_tampering_and_authenticates_reply() {
        let registration = Registration::parse(
            r#"{"version":1,"device":"001122334455","key":"abababababababababababababababababababababababababababababababab"}"#,
        ).unwrap();
        let server = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
        server
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let port = server.local_addr().unwrap().port();
        let key = registration.key;
        let worker = std::thread::spawn(move || {
            let mut request = [0; 128];
            let (length, address) = server.recv_from(&mut request).unwrap();
            assert_eq!(length, 58);
            assert_eq!(&request[..4], DISCOVER);
            assert_eq!(mac(&key, &request[..26]), request[26..58]);
            let mut reply = FOUND.to_vec();
            reply.extend(&request[4..26]);
            reply.extend(mac(&key, &reply));
            let mut bad = reply.clone();
            bad[57] ^= 1;
            server.send_to(&bad, address).unwrap();
            server.send_to(&reply, address).unwrap();
        });
        let start = Instant::now();
        let address = discover_with(
            &registration,
            2.,
            &["127.0.0.1"],
            port,
            || Ok([7; 16]),
            || start.elapsed().as_secs_f64(),
            Udp::new,
        )
        .unwrap();
        assert_eq!(address, "127.0.0.1");
        worker.join().unwrap();
    }
}
