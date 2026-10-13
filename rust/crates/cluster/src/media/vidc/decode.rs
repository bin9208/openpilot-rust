use super::{CaptureState, Decoder, Error, Queue, ReadyFrame, Result};
use crate::media::{frame::FrameLease, HardwareFrame};
use openpilot_encoderd::native::v4l::primitives::QueuedPlane;
use std::{sync::Arc, time::Instant};

impl Decoder {
    fn queue_input(&mut self, index: usize, packet: &[u8], sequence: u64) -> Result<()> {
        let input = self
            .inputs
            .get_mut(index)
            .ok_or(Error::Contract("invalid V4L2 decoder input buffer state"))?;
        if input.queued {
            return Err(Error::Contract("invalid V4L2 decoder input buffer state"));
        }
        if packet.is_empty() || packet.len() > input.buffer.length() {
            return Err(Error::Contract(
                "H264 access unit exceeds decoder input capacity",
            ));
        }
        input.buffer.write(packet)?;
        let input = &self.inputs[index];
        self.device()?.queue(
            Queue::Output,
            &input.buffer,
            QueuedPlane {
                index: u32::try_from(index)?,
                used: packet.len(),
                timestamp: sequence,
                timestamp_copy: true,
            },
        )?;
        self.inputs[index].queued = true;
        self.submitted.push_back(sequence);
        Ok(())
    }
    fn output(&mut self) -> Result<bool> {
        let Some(frame) = self.device()?.dequeue(Queue::Output)? else {
            return Ok(false);
        };
        let input = self.inputs.get_mut(frame.index).ok_or(Error::Contract(
            "V4L2 decoder returned invalid input buffer index",
        ))?;
        input.queued = false;
        Ok(true)
    }
    fn capture(&mut self) -> Result<bool> {
        let Some(frame) = self.device()?.dequeue(Queue::Capture)? else {
            return Ok(false);
        };
        let capture = self.captures.get_mut(frame.index).ok_or(Error::Contract(
            "V4L2 decoder returned invalid capture buffer index",
        ))?;
        if self.pending || frame.used == 0 {
            return Ok(true);
        }
        let mut sequence = frame.sequence;
        if let Some(submitted) = self.submitted.pop_front() {
            if sequence == 0 {
                sequence = submitted;
            }
        }
        capture.buffer.sync_from_device()?;
        if capture.state != CaptureState::Queued {
            return Err(Error::Contract(
                "V4L2 decoder capture buffer was not queued",
            ));
        }
        capture.state = CaptureState::Leased;
        self.ready.push_back(ReadyFrame {
            index: frame.index,
            sequence,
        });
        Ok(true)
    }
    fn process(&mut self, timeout_ms: i32) -> Result<bool> {
        let Some(ready) = self.device()?.poll(timeout_ms)? else {
            return Ok(false);
        };
        let mut progress = false;
        if ready.priority {
            while let Some(event) = self.device()?.event()? {
                self.handle_event(event)?;
                progress = true;
            }
        }
        if ready.output {
            while self.output()? {
                progress = true;
            }
        }
        if ready.input {
            while self.capture()? {
                progress = true;
            }
        }
        if ready.error && !progress && !ready.priority {
            return Err(Error::Contract("cluster H264 decoder reported POLLERR"));
        }
        Ok(progress)
    }
    fn take_ready(&mut self) -> Result<Arc<HardwareFrame>> {
        let ready = self
            .ready
            .pop_front()
            .ok_or(Error::Contract("H264 decoder has no completed frame"))?;
        let capture = &self.captures[ready.index];
        Ok(HardwareFrame::retained(
            capture.buffer.duplicate_fd()?,
            self.layout,
            FrameLease {
                index: u32::try_from(ready.index)?,
                generation: self.generation,
                sequence: ready.sequence,
                release: self.release.clone(),
                failures: self.failures.clone(),
            },
        )?)
    }
    /// # Errors
    /// Preserves malformed queue states and kernel failures; imports disable this owner.
    pub fn decode(&mut self, packet: &[u8], sequence: u64) -> Result<Option<Arc<HardwareFrame>>> {
        if !self.opened {
            return Err(Error::Contract("cluster H264 decoder is not open"));
        }
        while let Ok(reason) = self.failed.try_recv() {
            if self.disabled_reason.is_empty() {
                self.disabled_reason = reason;
            }
        }
        if !self.disabled_reason.is_empty() {
            return Err(Error::Contract(
                "TICI H264 decoder disabled after EGL import failure",
            ));
        }
        self.drain_releases()?;
        let deadline = Instant::now()
            .checked_add(self.config.timeout)
            .ok_or(Error::Contract("decoder timeout overflow"))?;
        let remaining = || {
            i32::try_from(
                deadline
                    .saturating_duration_since(Instant::now())
                    .as_millis(),
            )
            .unwrap_or(i32::MAX)
        };
        let index = loop {
            if let Some(index) = self.inputs.iter().position(|input| !input.queued) {
                break index;
            }
            if !self.process(remaining())? {
                return Ok(None);
            }
        };
        self.queue_input(index, packet, sequence)?;
        while self.ready.is_empty() {
            let timeout = remaining();
            if timeout == 0 || !self.process(timeout)? {
                return Ok(None);
            }
        }
        Ok(Some(self.take_ready()?))
    }
    #[must_use]
    pub fn disabled_reason(&self) -> &str {
        &self.disabled_reason
    }
}
