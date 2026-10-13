mod config;
mod controls;
mod queue;
mod setup;
use crate::renderer::nv12::Nv12Layout;
pub use config::{Config, InputFormat, Packet, Timings};
use openpilot_encoderd::{
    native::v4l::primitives::{Allocator, Buffer, Device, Queue},
    Error,
};
use std::{collections::VecDeque, os::fd::BorrowedFd, time::Instant};
pub type Result<T> = std::result::Result<T, Error>;
struct Input {
    buffer: Buffer,
    acquired: bool,
}
pub struct Encoder {
    config: Config,
    device: Option<Device>,
    allocator: Option<Allocator>,
    inputs: Vec<Input>,
    captures: Vec<Buffer>,
    free: VecDeque<usize>,
    layout: Nv12Layout,
    input_sizeimage: usize,
    capture_sizeimage: usize,
    streams_on: bool,
    opened: bool,
    sent_video: bool,
    codec_config: Vec<u8>,
    timings: Timings,
}
pub(super) fn elapsed(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX)
}
impl Encoder {
    /// # Errors
    /// Rejects source-invalid encoder dimensions, rates, controls, or device path.
    pub fn new(config: Config) -> Result<Self> {
        config.validate()?;
        let layout = Nv12Layout::venus(config.width, config.height)
            .map_err(|error| Error::Io(std::io::Error::other(error)))?;
        Ok(Self {
            config,
            device: None,
            allocator: None,
            inputs: Vec::new(),
            captures: Vec::new(),
            free: VecDeque::new(),
            layout,
            input_sizeimage: 0,
            capture_sizeimage: 0,
            streams_on: false,
            opened: false,
            sent_video: false,
            codec_config: Vec::new(),
            timings: Timings::default(),
        })
    }
    fn device(&self) -> Result<&Device> {
        self.device
            .as_ref()
            .ok_or(Error::Contract("cluster H264 encoder is not open"))
    }
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.opened
    }
    #[must_use]
    pub fn input_layout(&self) -> Nv12Layout {
        self.layout
    }
    #[must_use]
    pub fn input_sizeimage(&self) -> usize {
        self.input_sizeimage
    }
    #[must_use]
    pub fn capture_sizeimage(&self) -> usize {
        self.capture_sizeimage
    }
    #[must_use]
    pub fn last_timings(&self) -> Timings {
        self.timings
    }
    /// # Errors
    /// Preserves native setup errors and closes partially initialized buffers.
    pub fn open(&mut self) -> Result<()> {
        if self.opened {
            return Ok(());
        }
        self.device = Some(Device::open(&self.config.device_path)?);
        if let Err(error) = self.initialize() {
            self.close();
            return Err(error);
        }
        self.opened = true;
        Ok(())
    }
    /// # Errors
    /// Rejects an input that is not currently acquired; the callback borrows its mapping and FD.
    pub fn with_input<R>(
        &mut self,
        index: usize,
        callback: impl FnOnce(&mut [u8], BorrowedFd<'_>) -> R,
    ) -> Result<R> {
        let input = self
            .inputs
            .get_mut(index)
            .filter(|input| input.acquired)
            .ok_or(Error::Contract(
                "cluster H264 encoder input buffer is not acquired",
            ))?;
        Ok(input.buffer.with_input(callback))
    }
    pub fn close(&mut self) {
        if self.device.is_some() && self.opened {
            if let Some(device) = &self.device {
                let _ = device.encoder_stop();
            }
            let _ = self.drain(250, &mut |_| Ok(()));
        }
        if let Some(device) = &self.device {
            if self.streams_on {
                let _ = device.stream(Queue::Output, false);
                let _ = device.stream(Queue::Capture, false);
            }
            if device.buffers(Queue::Output, 0).is_ok() {
                let _ = device.buffers(Queue::Capture, 0);
            }
        }
        self.streams_on = false;
        self.device = None;
        self.inputs.clear();
        self.captures.clear();
        self.allocator = None;
        self.free.clear();
        self.codec_config.clear();
        self.sent_video = false;
        self.opened = false;
    }
}
impl Drop for Encoder {
    fn drop(&mut self) {
        self.close();
    }
}
