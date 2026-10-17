//! Safe, opt-in ZMQ boundary shared with the existing native logging crates.
use crate::seal::Result;
pub struct Zmq(::zmq::Context);
impl Zmq {
    pub fn new() -> Result<Self> {
        Ok(Self(::zmq::Context::new()))
    }
    pub fn socket(&self, endpoint: &str, pull: bool) -> Result<Socket> {
        let socket = self
            .0
            .socket(if pull { ::zmq::PULL } else { ::zmq::PUSH })?;
        socket.set_rcvtimeo(5000)?;
        socket.set_sndtimeo(5000)?;
        socket.set_linger(10)?;
        if pull {
            socket.bind(endpoint)?;
        } else {
            socket.connect(endpoint)?;
        }
        Ok(Socket(socket))
    }
}
pub struct Socket(::zmq::Socket);
impl Socket {
    pub fn send(&self, bytes: &[u8]) -> Result<()> {
        loop {
            match self.0.send(bytes, 0) {
                Err(::zmq::Error::EINTR) => {}
                result => return Ok(result?),
            }
        }
    }
    pub fn receive(&self) -> Result<Vec<u8>> {
        loop {
            match self.0.recv_bytes(0) {
                Err(::zmq::Error::EINTR) => {}
                result => return Ok(result?),
            }
        }
    }
}
