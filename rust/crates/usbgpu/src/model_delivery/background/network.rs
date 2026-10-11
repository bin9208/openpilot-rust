use crate::model_delivery::Error;
use std::{
    io,
    net::{TcpStream, ToSocketAddrs},
    thread,
    time::{Duration, Instant},
};

fn connect(host: &str, port: u16, timeout: Duration) -> io::Result<()> {
    let mut error = io::Error::new(
        io::ErrorKind::AddrNotAvailable,
        "no addresses for model host",
    );
    for address in (host, port).to_socket_addrs()? {
        match TcpStream::connect_timeout(&address, timeout) {
            Ok(connection) => {
                drop(connection);
                return Ok(());
            }
            Err(failed) => error = failed,
        }
    }
    Err(error)
}

pub(super) fn wait(manifest_url: &str, seconds: f64) -> Result<bool, Error> {
    let url = url::Url::parse(manifest_url).map_err(|error| Error::Invalid(error.to_string()))?;
    let host = url
        .host_str()
        .filter(|_| url.scheme() == "https")
        .ok_or_else(|| {
            Error::Invalid("model manifest URL must use HTTPS and include a host".into())
        })?
        .trim_start_matches('[')
        .trim_end_matches(']');
    let started = Instant::now();
    loop {
        let remaining = (seconds.max(0.0) - started.elapsed().as_secs_f64()).max(0.0);
        let bound = Duration::from_secs_f64(remaining.clamp(0.1, 2.0));
        if connect(host, url.port().unwrap_or(443), bound).is_ok() {
            return Ok(true);
        }
        let remaining = seconds.max(0.0) - started.elapsed().as_secs_f64();
        if remaining <= 0.0 {
            return Ok(false);
        }
        thread::sleep(Duration::from_secs_f64(remaining.min(2.0)));
    }
}
