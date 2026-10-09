use super::{tls::Shared, tls_poll::readable};
use rustls::{pki_types::ServerName, ClientConfig, ClientConnection, RootCertStore};
use std::{
    io::{self, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    sync::{atomic::Ordering, Arc},
    time::{Duration, Instant},
};

pub(super) fn connect(endpoint: &(String, u16)) -> io::Result<TcpStream> {
    let mut failure = io::Error::new(io::ErrorKind::NotFound, "host not found");
    for address in endpoint.to_socket_addrs()? {
        match TcpStream::connect_timeout(&address, Duration::from_secs(8)) {
            Ok(socket) => return Ok(socket),
            Err(error) => failure = error,
        }
    }
    Err(failure)
}
pub(super) fn relay(
    mut local: TcpStream,
    mut remote: TcpStream,
    host: String,
    shared: &Shared,
) -> io::Result<()> {
    let mut roots = RootCertStore::empty();
    let certificates = rustls_native_certs::load_native_certs();
    for cert in certificates.certs {
        let _accepted = roots.add(cert);
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(io::Error::other)?
        .with_root_certificates(roots)
        .with_no_client_auth();
    let name = ServerName::try_from(host).map_err(io::Error::other)?;
    let mut connection = ClientConnection::new(Arc::new(config), name).map_err(io::Error::other)?;
    let deadline = Instant::now() + Duration::from_secs(8);
    while connection.is_handshaking() {
        connection.complete_io(&mut super::tls_deadline::Handshake {
            socket: &mut remote,
            deadline,
        })?;
    }
    remote.set_read_timeout(Some(Duration::from_secs(6)))?;
    remote.set_write_timeout(Some(Duration::from_secs(6)))?;
    let mut buffer = vec![0; 64 * 1024];
    while !shared.stop.load(Ordering::SeqCst) {
        let ready = readable(&[&local, &remote], Duration::from_millis(500))?;
        if ready[0] {
            let length = local.read(&mut buffer)?;
            if length == 0 {
                return Ok(());
            }
            connection.writer().write_all(&buffer[..length])?;
            flush(&mut connection, &mut remote)?;
        }
        if ready[1] {
            if connection.read_tls(&mut remote)? == 0 {
                return Ok(());
            }
            connection.process_new_packets().map_err(io::Error::other)?;
            loop {
                match connection.reader().read(&mut buffer) {
                    Ok(0) => return Ok(()),
                    Ok(length) => local.write_all(&buffer[..length])?,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) => return Err(error),
                }
            }
            flush(&mut connection, &mut remote)?;
        }
    }
    Ok(())
}
fn flush(connection: &mut ClientConnection, remote: &mut TcpStream) -> io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(6);
    while connection.wants_write() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "write timed out"));
        }
        remote.set_write_timeout(Some(remaining))?;
        connection.write_tls(remote)?;
    }
    Ok(())
}
