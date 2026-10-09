pub mod bus_lock;
pub mod check;
pub mod clock;
pub mod hardware;
#[cfg(feature = "native-skip-miri")]
pub mod native_usb;
pub mod transport;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[cfg(feature = "native-skip-miri")]
    #[error(transparent)]
    Library(#[from] libloading::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("{operation}: {message}")]
    UsbApi {
        operation: &'static str,
        code: i32,
        message: String,
    },
    #[error("{0}")]
    Protocol(String),
    #[error("{0}")]
    Contract(&'static str),
    #[error("short eGPU power status ({0} bytes)")]
    ShortPower(usize),
    #[error("Cannot allocate {0} bytes")]
    Allocation(u64),
    #[error("GPU check cancelled")]
    Cancelled,
}

pub mod usb3;

pub mod custom_asm;

pub mod model;
