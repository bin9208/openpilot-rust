//! Typed delivery retry classes; certificate and artifact verification remain strict.
use super::Error;
use std::io;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Classification {
    pub code: &'static str,
    pub retryable: bool,
}
fn class(code: &'static str, retryable: bool) -> Classification {
    Classification { code, retryable }
}
pub fn http_status(status: u16) -> Classification {
    if matches!(status, 408 | 429 | 500..=599) {
        class("server", true)
    } else {
        class("download", false)
    }
}
pub fn tls(error: &rustls::Error) -> Classification {
    match error {
        rustls::Error::InvalidCertificate(
            rustls::CertificateError::NotValidYet
            | rustls::CertificateError::NotValidYetContext { .. },
        ) => class("clock", true),
        rustls::Error::InvalidCertificate(_) => class("certificate", false),
        _ => class("install", false),
    }
}
pub fn io(error: &io::Error) -> Classification {
    if let Some(inner) = error.get_ref() {
        if let Some(error) = inner.downcast_ref::<ureq::Error>() {
            return http(error);
        }
        if let Some(error) = inner.downcast_ref::<rustls::Error>() {
            return tls(error);
        }
    }
    if error.kind() == io::ErrorKind::StorageFull
        || error.raw_os_error() == Some(rustix::io::Errno::NOSPC.raw_os_error())
    {
        return class("storage", false);
    }
    // CPython constructs BrokenPipeError for Linux ESHUTDOWN as well as EPIPE.
    if error.raw_os_error() == Some(rustix::io::Errno::SHUTDOWN.raw_os_error()) {
        return class("network", true);
    }
    if matches!(
        error.kind(),
        io::ErrorKind::TimedOut
            | io::ErrorKind::ConnectionRefused
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::UnexpectedEof
            | io::ErrorKind::NetworkUnreachable
            | io::ErrorKind::HostUnreachable
    ) {
        return class("network", true);
    }
    class("install", false)
}
pub fn http(error: &ureq::Error) -> Classification {
    match error {
        ureq::Error::StatusCode(status) => http_status(*status),
        ureq::Error::HostNotFound => class("dns", true),
        ureq::Error::Timeout(_) | ureq::Error::ConnectionFailed => class("network", true),
        ureq::Error::Io(error) => io(error),
        ureq::Error::Rustls(error) => tls(error),
        _ => class("install", false),
    }
}
pub fn classify(error: &Error) -> Classification {
    match error {
        Error::Io(error) => io(error),
        Error::Http(error) => http(error),
        Error::HttpStatus { code, .. } => http_status(*code),
        Error::Native(crate::Error::Io(error)) => io(error),
        _ => class("install", false),
    }
}
