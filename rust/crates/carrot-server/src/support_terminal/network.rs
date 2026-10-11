use crate::Error;
use std::time::Duration;

fn builder() -> Result<reqwest::ClientBuilder, Error> {
    let roots = rustls_native_certs::load_native_certs()
        .certs
        .into_iter()
        .map(|cert| reqwest::Certificate::from_der(cert.as_ref()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| Error::Source(error.to_string()))?;
    Ok(reqwest::Client::builder().tls_certs_only(roots))
}
pub(super) fn client(shared: bool) -> Result<reqwest::Client, Error> {
    builder()?
        .timeout(Duration::from_secs(if shared { 300 } else { 12 }))
        .build()
        .map_err(|error| Error::Source(error.to_string()))
}
pub(super) fn download() -> Result<reqwest::Client, Error> {
    builder()?
        .connect_timeout(Duration::from_secs(60))
        .read_timeout(Duration::from_secs(60))
        .build()
        .map_err(|error| Error::Source(error.to_string()))
}
