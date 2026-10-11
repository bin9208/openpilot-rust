use super::{
    write::{FrameKind, NoAck},
    Display, Runtime,
};
use crate::Error;
use std::time::Duration;

#[derive(Clone, Copy)]
pub struct H264Chunk<'a> {
    pub bytes: &'a [u8],
    pub last: bool,
    pub wait_for_ack: bool,
    pub require_ack: bool,
}

impl<R: Runtime> Display<R> {
    /// # Errors
    /// Returns absence of a connection or failure to upload the transparent source overlay.
    pub fn start_h264_stream(&mut self, requested: i64) -> Result<u32, Error> {
        if self.connection.is_none() {
            return Err(Error::Contract("USB display is not open"));
        }
        let brightness = self.brightness_raw();
        for (id, fields, gap) in [
            (10, Vec::new(), 20),
            (111, Vec::new(), 20),
            (112, Vec::new(), 80),
            (13, vec![(8, i64::from(self.orientation))], 125),
            (14, vec![(8, brightness)], 360),
            (52, Vec::new(), 25),
        ] {
            self.optional_command(id, &fields, NoAck::NONE);
            self.runtime.sleep(Duration::from_millis(gap));
        }
        let bytes = super::overlay::transparent()?;
        self.drain(
            u32::try_from(self.options.frame_drain_attempts).unwrap_or(0),
            u32::try_from(self.options.frame_drain_timeout_ms).unwrap_or(0),
        );
        let mut payload = super::packet::frame(102, self.runtime.milliseconds()?, &bytes, false)?;
        self.write(&mut payload)?;
        self.runtime.sleep(Duration::from_millis(70));
        if self.options.display_fps > 0 {
            self.optional_command(15, &[(8, i64::from(self.options.display_fps))], NoAck::NONE);
            self.runtime.sleep(Duration::from_millis(15));
        }
        let mut size = self.chunk_size(requested);
        self.chunk_gap = Duration::ZERO;
        if self.runtime.gpu_active() {
            size = size.min(32 * 1024);
            self.chunk_gap = Duration::from_millis(2);
        }
        Ok(size)
    }

    pub fn stop_h264_stream(&mut self) {
        if self.connection.is_none() {
            return;
        }
        for id in [123, 122, 122] {
            self.optional_command(id, &[], NoAck::STATUS);
        }
    }

    /// # Errors
    /// Returns chunk write, required acknowledgement or disconnect failures.
    pub fn send_h264_chunk(&mut self, chunk: H264Chunk<'_>) -> Result<(), Error> {
        if self.connection.is_none() {
            return Err(Error::Contract("USB display is not open"));
        }
        if chunk.bytes.is_empty() {
            return Ok(());
        }
        let response = self.frame_payload(
            FrameKind::H264 { last: chunk.last },
            chunk.bytes,
            chunk.wait_for_ack,
        );
        match response {
            Err(cause) => {
                let error = self.io_error("TURZX USB H264 chunk timed out", cause);
                if super::write::disconnected(&error) || chunk.require_ack || !chunk.wait_for_ack {
                    return Err(error);
                }
                self.flow_control(2);
                return Ok(());
            }
            Ok(response) if chunk.wait_for_ack => {
                if chunk.require_ack {
                    if response.is_empty() {
                        return Err(Error::Contract("TURZX USB frame upload timed out"));
                    }
                    self.frame_errors = 0;
                } else {
                    self.flow_control(3);
                }
            }
            Ok(_) => {}
        }
        if !self.chunk_gap.is_zero() {
            self.runtime.sleep(self.chunk_gap);
        }
        Ok(())
    }

    fn chunk_size(&mut self, requested: i64) -> u32 {
        if requested > 0 {
            return u32::try_from(requested.min(1024 * 1024)).unwrap_or(1024 * 1024);
        }
        if self.runtime.gpu_active() {
            return 202_752;
        }
        match self.command_ack(17, &[]) {
            Ok(response) => {
                if let Some(bytes) = response
                    .get(8..12)
                    .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
                {
                    let value = u32::from_be_bytes(bytes);
                    if value > 0 && value <= 1024 * 1024 {
                        return value;
                    }
                }
            }
            Err(error) => self.runtime.warning(&format!(
                "Warning: TURZX H264 chunk-size negotiation failed: {error}"
            )),
        }
        202_752
    }

    fn flow_control(&mut self, target: u8) {
        for _ in 0..8 {
            let Ok(response) = self.command_ack(122, &[]) else {
                self.runtime.sleep(Duration::from_millis(50));
                return;
            };
            if response.get(8).is_none_or(|value| *value <= target) {
                return;
            }
            self.runtime.sleep(Duration::from_millis(50));
        }
    }
}
