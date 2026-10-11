use super::packet;
use crate::Error;
use std::time::Duration;

#[derive(Clone, Copy)]
pub(super) struct NoAck {
    pub gap: Duration,
    pub drains: u32,
}
impl NoAck {
    pub const NONE: Self = Self {
        gap: Duration::ZERO,
        drains: 0,
    };
    pub const FRAME_RATE: Self = Self {
        gap: Duration::from_millis(50),
        drains: 1,
    };
    pub const STATUS: Self = Self {
        gap: Duration::from_millis(80),
        drains: 1,
    };
}

pub(super) fn disconnected(error: &Error) -> bool {
    match error {
        Error::Disconnected(_)
        | Error::Usb(openpilot_usbgpu::Error::UsbApi { code: -4 | -1, .. }) => return true,
        Error::DisplayIo { cause, .. } => return disconnected(cause),
        Error::Io(error) if matches!(error.raw_os_error(), Some(19 | 6 | 5 | 108)) => return true,
        Error::Usb(_) | Error::Io(_) | Error::Contract(_) | Error::Json(_) | Error::Params(_) => {}
    }
    let message = error.to_string().to_lowercase();
    [
        "no such device",
        "device not found",
        "disconnected",
        "entity not found",
        "device has been disconnected",
    ]
    .iter()
    .any(|pattern| message.contains(pattern))
}

#[derive(Clone, Copy)]
pub(super) enum FrameKind {
    Jpeg,
    Png,
    H264 { last: bool },
}
impl FrameKind {
    pub(super) fn packet(self, timestamp: u32, bytes: &[u8]) -> Result<Vec<u8>, Error> {
        match self {
            Self::Jpeg => packet::frame(101, timestamp, bytes, false),
            Self::Png => packet::frame(102, timestamp, bytes, false),
            Self::H264 { last } => packet::frame(121, timestamp, bytes, last),
        }
    }
}
