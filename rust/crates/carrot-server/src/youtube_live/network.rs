use crate::Error;
use rustls::{pki_types::ServerName, ClientConfig, ClientConnection, RootCertStore};
use std::{
    io,
    net::{TcpStream, ToSocketAddrs},
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct Endpoint {
    pub base: String,
    pub host: String,
    pub port: u16,
}
impl Default for Endpoint {
    fn default() -> Self {
        Self {
            base: "rtmps://a.rtmps.youtube.com:443/live2".into(),
            host: "a.rtmps.youtube.com".into(),
            port: 443,
        }
    }
}
impl Endpoint {
    pub(super) fn tcp(&self, timeout: Duration) -> io::Result<TcpStream> {
        let mut last = io::Error::new(io::ErrorKind::NotFound, "host not found");
        for address in (self.host.as_str(), self.port).to_socket_addrs()? {
            match TcpStream::connect_timeout(&address, timeout) {
                Ok(socket) => return Ok(socket),
                Err(error) => last = error,
            }
        }
        Err(last)
    }
    pub(super) fn reachable(&self) -> bool {
        self.tcp(Duration::from_millis(1500)).is_ok()
    }
    pub(super) fn verify(&self) -> (bool, String) {
        match self.verify_tls() {
            Ok(()) => (true, "YouTube RTMPS ingest is reachable".into()),
            Err(error) => (false, format!("YouTube RTMPS ingest unreachable: {error}")),
        }
    }
    fn verify_tls(&self) -> Result<(), Error> {
        let mut socket = self.tcp(Duration::from_millis(2500))?;
        let mut roots = RootCertStore::empty();
        for cert in rustls_native_certs::load_native_certs().certs {
            let _cert = roots.add(cert);
        }
        let config =
            ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .map_err(|error| Error::Source(error.to_string()))?
                .with_root_certificates(roots)
                .with_no_client_auth();
        let name = ServerName::try_from(self.host.clone())
            .map_err(|error| Error::Source(error.to_string()))?;
        let mut connection = ClientConnection::new(Arc::new(config), name)
            .map_err(|error| Error::Source(error.to_string()))?;
        let deadline = Instant::now() + Duration::from_millis(2500);
        while connection.is_handshaking() {
            connection.complete_io(&mut super::tls_deadline::Handshake {
                socket: &mut socket,
                deadline,
            })?;
        }
        Ok(())
    }
}
