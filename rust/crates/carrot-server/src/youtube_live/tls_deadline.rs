//! rustls complete_io may issue many reads; each shares Python's total SSL handshake deadline.
use std::{
    io::{self, Read, Write},
    net::TcpStream,
    time::Instant,
};

pub(super) struct Handshake<'a> {
    pub socket: &'a mut TcpStream,
    pub deadline: Instant,
}
impl Handshake<'_> {
    fn prepare(&self) -> io::Result<()> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "handshake timed out",
            ));
        }
        self.socket.set_read_timeout(Some(remaining))?;
        self.socket.set_write_timeout(Some(remaining))
    }
}
impl Read for Handshake<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.prepare()?;
        self.socket.read(bytes)
    }
}
impl Write for Handshake<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.prepare()?;
        self.socket.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.prepare()?;
        self.socket.flush()
    }
}
