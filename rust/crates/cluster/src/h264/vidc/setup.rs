use super::{Encoder, Error, Input, Result};
use openpilot_encoderd::native::v4l::primitives::{Allocator, FormatKind, Queue, QueuedPlane};

impl Encoder {
    pub(super) fn initialize(&mut self) -> Result<()> {
        let capability = self.device()?.capability()?;
        if capability.driver != "msm_vidc_driver" || capability.card != "msm_vidc_venc" {
            eprintln!(
                "cluster H264 encoder is {}/{}, expected msm_vidc_driver/msm_vidc_venc",
                capability.driver, capability.card
            );
        }
        self.configure_formats()?;
        self.device()?
            .fps(Queue::Output, u32::try_from(self.config.fps)?)?;
        self.configure_controls()?;
        self.device()?.buffers(Queue::Capture, 6)?;
        self.device()?.buffers(Queue::Output, 7)?;
        let allocator = Allocator::open()?;
        for _ in 0..7 {
            let mut buffer = allocator.allocate(self.layout.byte_count)?;
            buffer.with_bytes_mut(|bytes| {
                let split = self.layout.uv_offset.min(bytes.len());
                bytes[..split].fill(16);
                bytes[split..].fill(128);
            });
            self.inputs.push(Input {
                buffer,
                acquired: false,
            });
        }
        for _ in 0..6 {
            self.captures
                .push(allocator.allocate(self.capture_sizeimage)?);
        }
        self.allocator = Some(allocator);
        self.device()?.stream(Queue::Capture, true)?;
        self.device()?.stream(Queue::Output, true)?;
        self.streams_on = true;
        for index in 0..6 {
            self.queue_capture(index)?;
        }
        self.free = (0..7).collect();
        self.codec_config.clear();
        self.sent_video = false;
        Ok(())
    }
    fn configure_formats(&mut self) -> Result<()> {
        let formats = self.device()?.formats(Queue::Output)?;
        let nv12 = u32::from_le_bytes(*b"NV12");
        if !formats.is_empty() && !formats.contains(&nv12) {
            return Err(Error::Io(std::io::Error::other(format!(
                "V4L2 encoder does not report NV12 input support; found: {}",
                formats
                    .into_iter()
                    .map(fourcc)
                    .collect::<Vec<_>>()
                    .join(", ")
            ))));
        }
        let dimensions = (
            u32::try_from(self.config.width)?,
            u32::try_from(self.config.height)?,
        );
        let capture =
            self.device()?
                .encoder_format(Queue::Capture, dimensions, FormatKind::H264)?;
        let input = self
            .device()?
            .encoder_format(Queue::Output, dimensions, FormatKind::Nv12)?;
        if input.pixel_format != nv12 {
            return Err(Error::Io(std::io::Error::other(format!(
                "V4L2 encoder rejected NV12 input, returned {}",
                fourcc(input.pixel_format)
            ))));
        }
        if capture.pixel_format != u32::from_le_bytes(*b"H264") {
            return Err(Error::Io(std::io::Error::other(format!(
                "V4L2 encoder rejected H264 output, returned {}",
                fourcc(capture.pixel_format)
            ))));
        }
        if (capture.width, capture.height) != dimensions
            || (input.width, input.height) != dimensions
        {
            return Err(Error::Contract(
                "V4L2 encoder adjusted dimensions; cluster H264 wrapper requires exact dimensions",
            ));
        }
        self.input_sizeimage = usize::try_from(input.size)?;
        self.layout.byte_count = self.layout.byte_count.max(self.input_sizeimage);
        self.capture_sizeimage = usize::try_from(capture.size)?;
        if self.capture_sizeimage == 0 {
            return Err(Error::Contract(
                "V4L2 encoder returned zero H264 capture sizeimage",
            ));
        }
        Ok(())
    }
    pub(super) fn queue_capture(&self, index: usize) -> Result<()> {
        let buffer = self.captures.get(index).ok_or(Error::Contract(
            "V4L2 encoder returned invalid capture buffer index",
        ))?;
        self.device()?.queue(
            Queue::Capture,
            buffer,
            QueuedPlane {
                index: u32::try_from(index)?,
                used: buffer.length(),
                timestamp: 0,
                timestamp_copy: false,
            },
        )
    }
}
fn fourcc(value: u32) -> String {
    String::from_utf8_lossy(&value.to_le_bytes())
        .split('\0')
        .next()
        .unwrap_or_default()
        .into()
}
