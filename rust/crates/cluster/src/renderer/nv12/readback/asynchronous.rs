use super::{
    state::{
        binding, Slot, GL_ALREADY_SIGNALED, GL_CONDITION_SATISFIED, GL_FRAMEBUFFER,
        GL_FRAMEBUFFER_BINDING, GL_PACK_ALIGNMENT, GL_PIXEL_PACK_BUFFER,
        GL_PIXEL_PACK_BUFFER_BINDING, GL_STREAM_READ, GL_SYNC_GPU_COMMANDS_COMPLETE,
        GL_TIMEOUT_EXPIRED, GL_WAIT_FAILED,
    },
    DirectReadback, Error, ReadTarget, Result,
};
use openpilot_startup_ui::renderer::TargetInfo;

impl DirectReadback {
    fn ensure_buffers(&mut self, bytes: usize) -> Result<()> {
        if !self.slots.is_empty() && self.byte_count == bytes {
            return Ok(());
        }
        self.close()?;
        let previous = binding(self.api.integer(GL_PIXEL_PACK_BUFFER_BINDING));
        self.clear_errors("asynchronous NV12 readback setup")?;
        let mut ids = Vec::new();
        let result = (|| -> Result<u32> {
            ids = self.api.generate_buffers(3)?;
            for buffer in &ids {
                if *buffer == 0 {
                    return Err(Error::InvalidBuffer);
                }
                self.api.bind_buffer(GL_PIXEL_PACK_BUFFER, *buffer);
                self.api.allocate_buffer(
                    GL_PIXEL_PACK_BUFFER,
                    u64::try_from(bytes).map_err(|_| Error::InvalidAsyncTarget)?,
                    GL_STREAM_READ,
                )?;
            }
            Ok(self.api.error())
        })();
        if result.is_err() {
            self.api.delete_buffers(&ids)?;
        }
        self.api.bind_buffer(GL_PIXEL_PACK_BUFFER, previous);
        let error = result?;
        if error != 0 {
            self.api.delete_buffers(&ids)?;
            return Err(Error::Gl {
                operation: "PBO allocation",
                code: error,
            });
        }
        self.slots = ids
            .into_iter()
            .map(|buffer| Slot {
                buffer,
                fence: None,
                bytes: 0,
                submitted: 0.0,
            })
            .collect();
        self.free = (0..self.slots.len()).collect();
        self.byte_count = bytes;
        Ok(())
    }
    /// # Errors
    /// Rejects unavailable asynchronous GL, invalid target metadata or submission failures.
    pub fn enqueue_rgba(&mut self, info: TargetInfo, now: f64) -> Result<bool> {
        if !self.enabled {
            return Err(Error::AsyncUnavailable);
        }
        let target = ReadTarget::try_from(info).map_err(|_| Error::InvalidAsyncTarget)?;
        let info = target.info;
        self.ensure_buffers(target.byte_count())?;
        if self.free.is_empty() {
            return Ok(false);
        }
        let framebuffer = binding(self.api.integer(GL_FRAMEBUFFER_BINDING));
        let alignment = self.api.integer(GL_PACK_ALIGNMENT);
        let buffer = binding(self.api.integer(GL_PIXEL_PACK_BUFFER_BINDING));
        self.clear_errors("asynchronous NV12 readback")?;
        let index = self.free.pop_front().ok_or(Error::InvalidBuffer)?;
        let slot = &mut self.slots[index];
        let result = (|| -> Result<u32> {
            self.api.bind_framebuffer(GL_FRAMEBUFFER, info.framebuffer);
            self.api.bind_buffer(GL_PIXEL_PACK_BUFFER, slot.buffer);
            self.api.pixel_store(GL_PACK_ALIGNMENT, 4);
            self.api.read_rgba_pbo((info.width, info.height))?;
            let fence = self.api.fence(GL_SYNC_GPU_COMMANDS_COMPLETE, 0)?;
            if fence == 0 {
                return Err(Error::FenceCreation);
            }
            slot.fence = Some(fence);
            slot.bytes = target.byte_count();
            slot.submitted = now;
            self.api.flush()?;
            Ok(self.api.error())
        })();
        if result.is_err() {
            if let Some(fence) = slot.fence.take() {
                self.api.delete_fence(fence)?;
            }
            self.free.push_front(index);
        }
        self.api.pixel_store(GL_PACK_ALIGNMENT, alignment);
        self.api.bind_buffer(GL_PIXEL_PACK_BUFFER, buffer);
        self.api.bind_framebuffer(GL_FRAMEBUFFER, framebuffer);
        let error = result?;
        if error != 0 {
            if let Some(fence) = slot.fence.take() {
                self.api.delete_fence(fence)?;
            }
            self.free.push_front(index);
            return Err(Error::Gl {
                operation: "PBO readback",
                code: error,
            });
        }
        self.pending.push_back(index);
        Ok(true)
    }
    /// # Errors
    /// Returns a lost fence, a wait failure or the source's strict one-second stall.
    pub fn async_ready(&mut self, now: f64) -> Result<bool> {
        let Some(index) = self.pending.front().copied() else {
            return Ok(false);
        };
        let slot = &self.slots[index];
        let fence = slot.fence.ok_or(Error::LostFence)?;
        match self.api.wait_fence(fence, 0, 0)? {
            GL_ALREADY_SIGNALED | GL_CONDITION_SATISFIED => Ok(true),
            GL_TIMEOUT_EXPIRED if now - slot.submitted > 1.0 => Err(Error::Stalled),
            GL_TIMEOUT_EXPIRED => Ok(false),
            GL_WAIT_FAILED => Err(Error::Gl {
                operation: "NV12 fence wait",
                code: self.api.error(),
            }),
            status => Err(Error::FenceStatus(status)),
        }
    }
    /// # Errors
    /// Rejects destination storage, failed fences or a failed/corrupted PBO mapping.
    pub fn copy_ready(&mut self, destination: &mut [u8], now: f64) -> Result<bool> {
        if !self.async_ready(now)? {
            return Ok(false);
        }
        let index = self.pending.front().copied().ok_or(Error::InvalidBuffer)?;
        let slot = &self.slots[index];
        if destination.len() < slot.bytes {
            return Err(Error::Destination {
                actual: destination.len(),
                required: slot.bytes,
                asynchronous: true,
            });
        }
        let previous = binding(self.api.integer(GL_PIXEL_PACK_BUFFER_BINDING));
        self.clear_errors("asynchronous NV12 readback copy")?;
        let (buffer, bytes) = (slot.buffer, slot.bytes);
        let result = (|| -> Result<u32> {
            self.api.bind_buffer(GL_PIXEL_PACK_BUFFER, buffer);
            let mapping = self
                .api
                .map_read(
                    GL_PIXEL_PACK_BUFFER,
                    u64::try_from(bytes).map_err(|_| Error::Mapping)?,
                )
                .map_err(|_| Error::Mapping)?;
            mapping.copy_to(destination)?;
            if !mapping.unmap() {
                return Err(Error::CorruptedMapping);
            }
            Ok(self.api.error())
        })();
        self.api.bind_buffer(GL_PIXEL_PACK_BUFFER, previous);
        let error = result?;
        if error != 0 {
            return Err(Error::Gl {
                operation: "PBO copy",
                code: error,
            });
        }
        let slot = &mut self.slots[index];
        if let Some(fence) = slot.fence.take() {
            self.api.delete_fence(fence)?;
        }
        slot.bytes = 0;
        slot.submitted = 0.0;
        self.pending.pop_front();
        self.free.push_back(index);
        Ok(true)
    }
}
