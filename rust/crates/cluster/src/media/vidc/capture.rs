use super::{Capture, CaptureState, Decoder, Error, Queue, Result};
use openpilot_encoderd::native::v4l::primitives::{Event, FormatKind, QueuedPlane};

impl Decoder {
    pub(super) fn configure_capture(&mut self) -> Result<()> {
        self.device()?
            .format(Queue::Capture, self.coded, FormatKind::Nv12, false)?;
        let format = self
            .device()?
            .format(Queue::Capture, self.coded, FormatKind::Nv12, true)?;
        if format.pixel_format != u32::from_le_bytes(*b"NV12") {
            return Err(Error::Contract(
                "V4L2 decoder rejected linear NV12 capture format",
            ));
        }
        let venus = crate::renderer::nv12::Nv12Layout::venus(
            i32::try_from(self.coded.0)?,
            i32::try_from(self.coded.1)?,
        )
        .map_err(|_| Error::Contract("decoder Venus layout invalid"))?;
        let stride = usize::try_from(format.stride)?.max(usize::try_from(venus.stride)?);
        let uv_offset = stride
            .checked_mul(usize::try_from(venus.y_scanlines)?)
            .ok_or(Error::Contract("decoder UV offset overflow"))?;
        let active = uv_offset
            .checked_add(
                stride
                    .checked_mul(usize::try_from(venus.uv_scanlines)?)
                    .ok_or(Error::Contract("decoder UV size overflow"))?,
            )
            .ok_or(Error::Contract("decoder active size overflow"))?;
        self.layout = openpilot_startup_ui::egl::FrameLayout {
            width: self.config.width,
            height: self.config.height,
            stride: i32::try_from(stride)?,
            uv_offset: i32::try_from(uv_offset)?,
        };
        self.capture_size = usize::try_from(format.size)?
            .max(active)
            .max(venus.byte_count);
        Ok(())
    }
    pub(super) fn allocate_capture(&mut self) -> Result<()> {
        let count = self
            .device()?
            .minimum_capture_buffers()
            .map_or(10, |value| value.clamp(10, 32));
        let count = self.request(Queue::Capture, count)?;
        let allocator = self
            .allocator
            .as_ref()
            .ok_or(Error::Contract("decoder allocator missing"))?;
        for _ in 0..count {
            self.captures.push(Capture {
                buffer: allocator.allocate(self.capture_size)?,
                state: CaptureState::Unallocated,
            });
        }
        self.device()?.stream(Queue::Capture, true)?;
        self.streams.capture = true;
        for index in 0..self.captures.len() {
            self.queue_capture(index)?;
        }
        Ok(())
    }
    fn queue_capture(&mut self, index: usize) -> Result<()> {
        let capture = self
            .captures
            .get(index)
            .ok_or(Error::Contract("invalid V4L2 decoder capture buffer state"))?;
        if capture.state == CaptureState::Queued {
            return Err(Error::Contract("invalid V4L2 decoder capture buffer state"));
        }
        self.device()?.queue(
            Queue::Capture,
            &capture.buffer,
            QueuedPlane {
                index: u32::try_from(index)?,
                used: capture.buffer.length(),
                timestamp: 0,
                timestamp_copy: false,
            },
        )?;
        self.captures[index].state = CaptureState::Queued;
        Ok(())
    }
    fn release(&mut self, index: usize) -> Result<()> {
        if !self.opened || index >= self.captures.len() {
            return Ok(());
        }
        if self.captures[index].state != CaptureState::Leased {
            return Err(Error::Contract(
                "H264 decoder capture buffer was released twice",
            ));
        }
        if self.pending {
            self.captures[index].state = CaptureState::Unallocated;
            Ok(())
        } else {
            self.queue_capture(index)
        }
    }
    /// # Errors
    /// Preserves requeue errors after the last owned frame releases its lease.
    pub fn drain_releases(&mut self) -> Result<()> {
        while let Ok(index) = self.released.try_recv() {
            self.release(usize::try_from(index)?)?;
        }
        Ok(())
    }
    pub(super) fn handle_event(&mut self, event: Event) -> Result<()> {
        match event {
            Event::PortChanged { width, height } => {
                self.coded = (width.max(1), height.max(1));
                if self
                    .captures
                    .iter()
                    .any(|capture| capture.state == CaptureState::Leased)
                {
                    return Err(Error::Contract(
                        "H264 decoder format changed while capture buffers were leased",
                    ));
                }
                self.device()?.flush_capture()?;
                self.pending = true;
            }
            Event::Flush { capture } => {
                if self.pending && capture {
                    self.restart_capture()?;
                    self.pending = false;
                }
            }
            Event::Other(_) => {}
        }
        Ok(())
    }
    fn restart_capture(&mut self) -> Result<()> {
        if self
            .captures
            .iter()
            .any(|capture| capture.state == CaptureState::Leased)
        {
            return Err(Error::Contract(
                "cannot restart H264 decoder with leased capture buffers",
            ));
        }
        self.ready.clear();
        if self.streams.capture {
            if let Err(error) = self.device()?.stream(Queue::Capture, false) {
                eprintln!("cluster decoder capture restart: {error}");
            }
            self.streams.capture = false;
        }
        self.request(Queue::Capture, 0)?;
        self.captures.clear();
        self.device()?.decoder_dpb()?;
        self.configure_capture()?;
        self.allocate_capture()
    }
}
