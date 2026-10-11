use crate::Error;
use std::time::Duration;

pub(super) fn client(shared: bool) -> Result<reqwest::Client, Error> {
    crate::native_http::builder()?
        .timeout(Duration::from_secs(if shared { 300 } else { 12 }))
        .build()
        .map_err(|error| Error::Source(error.to_string()))
}
pub(super) fn download() -> Result<reqwest::Client, Error> {
    crate::native_http::builder()?
        .connect_timeout(Duration::from_secs(60))
        .read_timeout(Duration::from_secs(60))
        .build()
        .map_err(|error| Error::Source(error.to_string()))
}
