use super::{elapsed, Encoder, Error, Packet, Result, Timings};
use openpilot_encoderd::native::v4l::primitives::{Queue, QueuedPlane};
use std::time::Instant;
type Callback<'a> = dyn FnMut(&Packet) -> Result<()> + 'a;
impl Encoder {
    fn process(
        &mut self,
        mut timeout_ms: i32,
        stop: bool,
        callback: &mut Callback<'_>,
    ) -> Result<usize> {
        let mut count = 0;
        loop {
            let Some(ready) = self.device()?.encoder_poll(timeout_ms)? else {
                return Ok(count);
            };
            if ready.error && !ready.input && !ready.output {
                return Err(Error::Contract(
                    "cluster H264 V4L2 encoder reported POLLERR",
                ));
            }
            let mut progress = false;
            if ready.input {
                while let Some(frame) = self.device()?.encoder_dequeue(Queue::Capture)? {
                    progress = true;
                    if !frame.eos() && frame.used > 0 {
                        let buffer = self.captures.get(frame.index).ok_or(Error::Contract(
                            "V4L2 encoder returned invalid capture buffer index",
                        ))?;
                        buffer.sync_from_device()?;
                        let data = buffer.copy_out(frame.used)?;
                        if frame.codec_config() {
                            self.codec_config = data;
                            count += 1;
                            self.queue_capture(frame.index)?;
                            continue;
                        }
                        let mut joined = if !self.codec_config.is_empty()
                            && (frame.keyframe() || !self.sent_video)
                        {
                            self.codec_config.clone()
                        } else {
                            Vec::new()
                        };
                        joined.extend_from_slice(&data);
                        callback(&Packet {
                            data: joined,
                            flags: frame.flags,
                            timestamp_us: frame.sequence,
                            codec_config: false,
                            keyframe: frame.keyframe(),
                        })?;
                        self.sent_video = true;
                        count += 1;
                    }
                    self.queue_capture(frame.index)?;
                }
            }
            if ready.output {
                while let Some(frame) = self.device()?.encoder_dequeue(Queue::Output)? {
                    progress = true;
                    if !self.free.contains(&frame.index) {
                        self.free.push_back(frame.index);
                    }
                }
            }
            if stop || !progress {
                return Ok(count);
            }
            timeout_ms = 0;
        }
    }
    /// # Errors
    /// Preserves capture, queue, and callback failures while draining native packets.
    pub fn drain(&mut self, timeout_ms: i32, callback: &mut Callback<'_>) -> Result<()> {
        if self.opened {
            self.process(timeout_ms, false, callback)?;
        }
        Ok(())
    }
    /// # Errors
    /// Rejects closed or unallocated inputs and preserves the source two-second wait failure.
    pub fn acquire(&mut self, callback: &mut Callback<'_>) -> Result<usize> {
        if !self.opened {
            return Err(Error::Contract("cluster H264 encoder is not open"));
        }
        self.timings = Timings::default();
        let start = Instant::now();
        self.process(0, false, callback)?;
        self.timings.pre_poll_us = elapsed(start);
        while self.free.is_empty() {
            let start = Instant::now();
            let packets = self.process(2000, true, callback)?;
            self.timings.wait_input_us = self.timings.wait_input_us.wrapping_add(elapsed(start));
            if self.free.is_empty() && packets == 0 {
                return Err(Error::Contract(
                    "cluster H264 encoder timed out waiting for a free input buffer",
                ));
            }
        }
        let index = self
            .free
            .pop_front()
            .ok_or(Error::Contract("cluster H264 encoder has no free input"))?;
        let Some(input) = self
            .inputs
            .get_mut(index)
            .filter(|input| input.buffer.length() >= self.layout.byte_count)
        else {
            self.free.push_front(index);
            return Err(Error::Contract(
                "cluster H264 encoder input buffer is not allocated",
            ));
        };
        input.acquired = true;
        Ok(index)
    }
    /// # Errors
    /// Rejects a slot not currently acquired from this encoder.
    pub fn cancel(&mut self, index: usize) -> Result<()> {
        let input = self
            .inputs
            .get_mut(index)
            .filter(|input| input.acquired)
            .ok_or(Error::Contract(
                "cluster H264 encoder input buffer is not acquired",
            ))?;
        input.acquired = false;
        if !self.free.contains(&index) {
            self.free.push_front(index);
        }
        Ok(())
    }
    /// # Errors
    /// Preserves cache synchronization, QBUF, and packet callback failures; unqueued inputs are returned.
    pub fn submit(
        &mut self,
        index: usize,
        timestamp_us: u64,
        dmabuf_written: bool,
        callback: &mut Callback<'_>,
    ) -> Result<()> {
        if self.inputs.get(index).is_none_or(|input| !input.acquired) {
            return Err(Error::Contract(
                "cluster H264 encoder input buffer is not acquired",
            ));
        }
        let result = (|| {
            let start = Instant::now();
            let input = &self.inputs[index];
            if dmabuf_written {
                input.buffer.sync_from_device()?;
            }
            input.buffer.sync_to_device()?;
            self.timings.sync_us = elapsed(start);
            let start = Instant::now();
            self.device()?.queue(
                Queue::Output,
                &self.inputs[index].buffer,
                QueuedPlane {
                    index: u32::try_from(index)?,
                    used: self.layout.byte_count,
                    timestamp: timestamp_us,
                    timestamp_copy: true,
                },
            )?;
            self.timings.queue_us = elapsed(start);
            self.inputs[index].acquired = false;
            let start = Instant::now();
            self.process(0, false, callback)?;
            self.timings.post_poll_us = elapsed(start);
            self.timings.total_us = [
                self.timings.pre_poll_us,
                self.timings.wait_input_us,
                self.timings.convert_us,
                self.timings.sync_us,
                self.timings.queue_us,
                self.timings.post_poll_us,
            ]
            .into_iter()
            .fold(0, u64::wrapping_add);
            Ok(())
        })();
        if result.is_err() && self.inputs[index].acquired {
            self.cancel(index)?;
        }
        result
    }
    /// # Errors
    /// Rejects incomplete packed input and preserves acquisition/submission failures.
    pub fn encode_nv12(
        &mut self,
        bytes: &[u8],
        timestamp_us: u64,
        callback: &mut Callback<'_>,
    ) -> Result<()> {
        self.encode_copy(
            bytes,
            timestamp_us,
            self.layout.byte_count,
            callback,
            "cluster H264 encoder NV12 input is smaller than the V4L2 input bytesused",
        )
    }
    /// # Errors
    /// Rejects incomplete active planes while preserving the existing padded input tail.
    pub fn encode_nv12_active(
        &mut self,
        bytes: &[u8],
        timestamp_us: u64,
        callback: &mut Callback<'_>,
    ) -> Result<()> {
        let active = self
            .layout
            .uv_offset
            .checked_add(
                usize::try_from(self.layout.stride)?
                    .checked_mul(usize::try_from(self.layout.uv_scanlines)?)
                    .ok_or(Error::Contract("NV12 active size overflow"))?,
            )
            .ok_or(Error::Contract("NV12 active size overflow"))?;
        self.encode_copy(
            bytes,
            timestamp_us,
            active,
            callback,
            "cluster H264 encoder active NV12 input is smaller than the Y+UV plane bytes",
        )
    }
    fn encode_copy(
        &mut self,
        bytes: &[u8],
        timestamp_us: u64,
        size: usize,
        callback: &mut Callback<'_>,
        message: &'static str,
    ) -> Result<()> {
        let index = self.acquire(callback)?;
        let start = Instant::now();
        if bytes.len() < size {
            self.cancel(index)?;
            return Err(Error::Contract(message));
        }
        self.inputs[index]
            .buffer
            .with_bytes_mut(|destination| destination[..size].copy_from_slice(&bytes[..size]));
        self.timings.convert_us = elapsed(start);
        self.submit(index, timestamp_us, false, callback)
    }
}
