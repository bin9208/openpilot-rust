use super::{packet, Display, Runtime};
use crate::Error;
use openpilot_usbgpu::transport::Setup;
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
        Error::Usb(_) | Error::Io(_) | Error::Contract(_) | Error::Json(_) => {}
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
    fn packet(self, timestamp: u32, bytes: &[u8]) -> Result<Vec<u8>, Error> {
        match self {
            Self::Jpeg => packet::frame(101, timestamp, bytes, false),
            Self::Png => packet::frame(102, timestamp, bytes, false),
            Self::H264 { last } => packet::frame(121, timestamp, bytes, last),
        }
    }
}

impl<R: Runtime> Display<R> {
    pub(super) fn clear_halt(&mut self) {
        if let Some(connection) = self.connection.as_mut() {
            for endpoint in [connection.output(), connection.input()]
                .into_iter()
                .flatten()
            {
                // The original cleanup ignores per-endpoint clear errors and still attempts the other endpoint.
                if let Err(_error) = connection.setup(Setup::ClearHalt, i32::from(endpoint)) {}
            }
        }
    }

    pub(super) fn drain(&mut self, attempts: u32, timeout: u32) {
        if let Some(connection) = self.connection.as_mut() {
            let Some(input) = connection.input() else {
                return;
            };
            for _ in 0..attempts {
                if connection.bulk(input, &mut [0; 512], timeout).is_err() {
                    return;
                }
            }
        }
    }

    pub(super) fn write(&mut self, payload: &mut [u8]) -> Result<(), Error> {
        let connection = self
            .connection
            .as_mut()
            .ok_or(Error::Contract("USB OUT endpoint is not open"))?;
        let output = connection
            .output()
            .ok_or(Error::Contract("USB OUT endpoint is not open"))?;
        connection.bulk(output, payload, 2000)?;
        Ok(())
    }

    pub(super) fn read_ack(&mut self) -> Result<Vec<u8>, Error> {
        let connection = self
            .connection
            .as_mut()
            .ok_or(Error::Contract("USB endpoints are not open"))?;
        let mut response = [0; 512];
        let input = connection
            .input()
            .ok_or(Error::Contract("USB endpoints are not open"))?;
        let count = connection.bulk(input, &mut response, 2000)?;
        Ok(response[..count].to_vec())
    }

    pub(super) fn io_error(&mut self, message: &'static str, cause: Error) -> Error {
        if disconnected(&cause) {
            self.connection = None;
            self.runtime.connected(false);
            Error::Disconnected(Box::new(cause))
        } else {
            Error::DisplayIo {
                message,
                cause: Box::new(cause),
            }
        }
    }

    pub(super) fn command_ack(
        &mut self,
        id: u8,
        fields: &[(usize, i64)],
    ) -> Result<Vec<u8>, Error> {
        let mut payload = packet::command(id, self.runtime.milliseconds()?, fields)?;
        self.clear_halt();
        self.drain(3, 20);
        self.write(&mut payload)
            .and_then(|()| self.read_ack())
            .map_err(|cause| self.io_error("TURZX USB command timed out", cause))
    }

    pub(super) fn optional_command(&mut self, id: u8, fields: &[(usize, i64)], pacing: NoAck) {
        let result = self
            .runtime
            .milliseconds()
            .and_then(|timestamp| packet::command(id, timestamp, fields))
            .and_then(|mut payload| {
                self.clear_halt();
                self.drain(3, 20);
                self.write(&mut payload)
            });
        match result {
            Ok(()) => {
                if !pacing.gap.is_zero() {
                    self.runtime.sleep(pacing.gap);
                }
                self.drain(pacing.drains, 20);
            }
            Err(cause) => {
                let error = self.io_error("TURZX USB command write failed", cause);
                self.runtime.warning(&format!(
                    "Warning: optional TURZX USB command skipped: {error}"
                ));
            }
        }
    }

    /// # Errors
    /// Returns disconnect or terminal frame rejection; other failures attempt source recovery.
    pub fn send_jpeg(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.send_frame(FrameKind::Jpeg, bytes)
    }

    /// # Errors
    /// Returns disconnect or terminal frame rejection; other failures attempt source recovery.
    pub fn send_png(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.send_frame(FrameKind::Png, bytes)
    }

    fn send_frame(&mut self, kind: FrameKind, bytes: &[u8]) -> Result<(), Error> {
        if self.connection.is_none() {
            return Err(Error::Contract("USB display is not open"));
        }
        let mut result = self.frame_payload(kind, bytes, self.options.wait_for_frame_ack);
        if self.options.wait_for_frame_ack && !self.options.fast_write {
            result =
                result.map_err(|cause| self.io_error("TURZX USB frame upload timed out", cause));
        }
        let result = result.and_then(|response| {
            if self.options.wait_for_frame_ack && response.is_empty() {
                return Err(Error::Contract("TURZX USB frame upload timed out"));
            }
            self.frame_errors = 0;
            Ok(())
        });
        match result {
            Ok(()) => Ok(()),
            Err(error) => self.frame_error(error),
        }
    }

    pub(super) fn frame_payload(
        &mut self,
        kind: FrameKind,
        bytes: &[u8],
        ack: bool,
    ) -> Result<Vec<u8>, Error> {
        let prepare = match kind {
            FrameKind::Jpeg | FrameKind::Png => ack,
            FrameKind::H264 { .. } => false,
        };
        let packet_first = prepare && !self.options.fast_write;
        let packet = if packet_first {
            Some(kind.packet(self.runtime.milliseconds()?, bytes)?)
        } else {
            None
        };
        if prepare {
            self.clear_halt();
            self.drain(3, 20);
        } else if !ack {
            let (attempts, timeout) = if self.options.fast_write {
                (
                    self.options.fast_frame_drain_attempts,
                    self.options.fast_frame_drain_timeout_ms,
                )
            } else {
                (
                    self.options.frame_drain_attempts,
                    self.options.frame_drain_timeout_ms,
                )
            };
            self.drain(
                u32::try_from(attempts).unwrap_or(0),
                u32::try_from(timeout).unwrap_or(0),
            );
        }
        let mut payload = match packet {
            Some(packet) => packet,
            None => kind.packet(self.runtime.milliseconds()?, bytes)?,
        };
        self.write(&mut payload)?;
        if ack {
            self.read_ack()
        } else {
            Ok(Vec::new())
        }
    }
}
