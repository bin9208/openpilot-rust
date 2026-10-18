use super::{write::FrameKind, Display, Runtime};
use crate::Error;

struct Stages {
    prepare: Option<&'static str>,
    payload: Option<&'static str>,
    write: &'static str,
    read: &'static str,
}

fn stages(kind: FrameKind, ack: bool, fast: bool) -> Stages {
    match (kind, ack, fast) {
        (FrameKind::H264 { .. }, true, _) => Stages {
            prepare: None,
            payload: Some("usb.h264.payload"),
            write: "usb.h264.write",
            read: "usb.h264.read_ack",
        },
        (FrameKind::H264 { .. }, false, _) => Stages {
            prepare: Some("usb.h264_no_ack.drain_input"),
            payload: Some("usb.h264_no_ack.payload"),
            write: "usb.h264_no_ack.write",
            read: "usb.h264.read_ack",
        },
        (_, false, _) => Stages {
            prepare: Some("usb.frame_no_ack.drain_input"),
            payload: Some("usb.frame_no_ack.payload"),
            write: "usb.frame_no_ack.write",
            read: "usb.frame_fast.read_ack",
        },
        (_, true, true) => Stages {
            prepare: Some("usb.frame_fast.prepare"),
            payload: Some("usb.frame_fast.payload"),
            write: "usb.frame_fast.write",
            read: "usb.frame_fast.read_ack",
        },
        (_, true, false) => Stages {
            prepare: Some("usb.write_checked.prepare"),
            payload: None,
            write: "usb.write_checked.write",
            read: "usb.write_checked.read_ack",
        },
    }
}

impl<R: Runtime> Display<R> {
    /// # Errors
    /// Returns disconnect or terminal frame rejection; other failures attempt source recovery.
    pub fn send_jpeg(&mut self, bytes: &[u8]) -> Result<(), Error> {
        if self.wifi.is_some() {
            let payload = self.build_payload(FrameKind::Jpeg, bytes)?;
            return self
                .wifi
                .as_mut()
                .ok_or(Error::Contract("S3 display is closed"))?
                .transaction(&payload, true, true, &mut self.runtime)
                .map(|_| ());
        }
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
        self.frame_payload_with_drain(kind, bytes, ack, self.options.fast_write)
    }

    fn build_payload(&mut self, kind: FrameKind, bytes: &[u8]) -> Result<Vec<u8>, Error> {
        let start = self.profile.start();
        let payload = kind.packet(self.runtime.milliseconds()?, bytes)?;
        self.profile.add(
            match kind {
                FrameKind::Jpeg | FrameKind::Png => "usb.frame.build_payload",
                FrameKind::H264 { .. } => "usb.h264.build_payload",
            },
            start,
        );
        Ok(payload)
    }

    pub(super) fn frame_payload_with_drain(
        &mut self,
        kind: FrameKind,
        bytes: &[u8],
        ack: bool,
        fast_drain: bool,
    ) -> Result<Vec<u8>, Error> {
        if self.wifi.is_some() {
            let payload = self.build_payload(kind, bytes)?;
            return self
                .wifi
                .as_mut()
                .expect("selected Wi-Fi transport")
                .transaction(&payload, false, false, &mut self.runtime);
        }
        let names = stages(kind, ack, self.options.fast_write);
        let prepare = match kind {
            FrameKind::Jpeg | FrameKind::Png => ack,
            FrameKind::H264 { .. } => false,
        };
        let packet_first = prepare && !self.options.fast_write;
        let packet = if packet_first {
            Some(self.build_payload(kind, bytes)?)
        } else {
            None
        };
        self.endpoints(ack)?;
        let start = self.profile.start();
        if prepare {
            self.clear_halt();
            self.drain(3, 20);
        } else if !ack {
            let (attempts, timeout) = if fast_drain {
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
        if let Some(name) = names.prepare {
            self.profile.add(name, start);
        }
        let start = self.profile.start();
        let mut payload = match packet {
            Some(packet) => packet,
            None => self.build_payload(kind, bytes)?,
        };
        if let Some(name) = names.payload {
            self.profile.add(name, start);
        }
        let start = self.profile.start();
        self.write(&mut payload)?;
        self.profile.add(names.write, start);
        if ack {
            let start = self.profile.start();
            let response = self.read_ack()?;
            self.profile.add(names.read, start);
            Ok(response)
        } else {
            Ok(Vec::new())
        }
    }
}
