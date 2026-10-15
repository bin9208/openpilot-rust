use crate::{Connection, Error, Registration, Socket};
use std::{
    io::{self, Read, Write},
    net::{Shutdown, TcpStream, ToSocketAddrs},
    time::{Duration, Instant},
};
const TIMEOUT: Duration = Duration::from_secs(3);
fn timed_out(error: io::Error) -> io::Error {
    if matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    ) {
        io::Error::new(io::ErrorKind::TimedOut, "timed out")
    } else {
        error
    }
}
pub struct TcpSocket(Option<TcpStream>);
impl TcpSocket {
    fn stream(&self) -> io::Result<&TcpStream> {
        self.0
            .as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "S3 display disconnected"))
    }
    fn stream_mut(&mut self) -> io::Result<&mut TcpStream> {
        self.0
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "S3 display disconnected"))
    }
}
impl Read for TcpSocket {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.stream_mut()?.read(bytes).map_err(timed_out)
    }
}
impl Write for TcpSocket {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.stream_mut()?.write(bytes).map_err(timed_out)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.stream_mut()?.flush()
    }
    fn write_all(&mut self, mut bytes: &[u8]) -> io::Result<()> {
        // Python socket.sendall's timeout bounds the whole operation, rather
        // than restarting three seconds after each partial write.
        let deadline = Instant::now() + TIMEOUT;
        let stream = self.stream_mut()?;
        let result = (|| {
            while !bytes.is_empty() {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(io::Error::new(io::ErrorKind::TimedOut, "timed out"));
                }
                stream.set_write_timeout(Some(remaining))?;
                match stream.write(bytes) {
                    Ok(0) => {
                        return Err(io::Error::new(
                            io::ErrorKind::WriteZero,
                            "failed to send S3 payload",
                        ))
                    }
                    Ok(size) => bytes = &bytes[size..],
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                    Err(error) => return Err(timed_out(error)),
                }
            }
            Ok(())
        })();
        let restored = stream.set_write_timeout(Some(TIMEOUT));
        result.and(restored)
    }
}
impl Socket for TcpSocket {
    fn nodelay(&mut self) -> io::Result<()> {
        self.stream()?.set_nodelay(true)
    }
    fn close(&mut self) -> io::Result<()> {
        if let Some(stream) = self.0.take() {
            let _ = stream.shutdown(Shutdown::Both);
        }
        Ok(())
    }
}
/// Connect to an explicit discovered/selected host. Discovery is a separate
/// source phase and is never silently replaced with an unauthenticated host.
pub fn connect(
    registration: &Registration,
    host: &str,
    port: u16,
) -> Result<Connection<TcpSocket>, Error> {
    let addresses = (host, port).to_socket_addrs()?;
    let mut last = io::Error::new(
        io::ErrorKind::AddrNotAvailable,
        "getaddrinfo returns an empty list",
    );
    for address in addresses {
        match TcpStream::connect_timeout(&address, TIMEOUT) {
            Ok(stream) => {
                stream.set_read_timeout(Some(TIMEOUT))?;
                stream.set_write_timeout(Some(TIMEOUT))?;
                return Connection::establish(TcpSocket(Some(stream)), registration, || {
                    let mut nonce = [0; 32];
                    getrandom::fill(&mut nonce)
                        .map_err(|error| io::Error::other(error.to_string()))?;
                    Ok(nonce)
                });
            }
            Err(error) => last = timed_out(error),
        }
    }
    Err(last.into())
}
