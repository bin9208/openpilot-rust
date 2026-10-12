use crate::Error;

pub(crate) fn builder() -> Result<reqwest::ClientBuilder, Error> {
    let roots = rustls_native_certs::load_native_certs()
        .certs
        .into_iter()
        .map(|cert| reqwest::Certificate::from_der(cert.as_ref()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| Error::Source(error.to_string()))?;
    Ok(reqwest::Client::builder().tls_certs_only(roots))
}
