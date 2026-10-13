mod asynchronous;
mod error;
mod state;

pub use error::{Error, Result};
use openpilot_startup_ui::{gles::GlesApi, renderer::TargetInfo};
use state::{
    binding, Slot, GL_FRAMEBUFFER, GL_FRAMEBUFFER_BINDING, GL_PACK_ALIGNMENT, GL_PIXEL_PACK_BUFFER,
    GL_PIXEL_PACK_BUFFER_BINDING,
};
use std::{collections::VecDeque, path::Path};

#[derive(Clone, Copy)]
pub struct ReadTarget {
    pub(super) info: TargetInfo,
    bytes: usize,
}
impl TryFrom<TargetInfo> for ReadTarget {
    type Error = Error;
    fn try_from(info: TargetInfo) -> Result<Self> {
        if info.framebuffer == 0 || info.width <= 0 || info.height <= 0 {
            return Err(Error::InvalidTarget);
        }
        let bytes = usize::try_from(info.width)
            .ok()
            .and_then(|width| {
                usize::try_from(info.height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|size| size.checked_mul(4))
            .ok_or(Error::InvalidTarget)?;
        Ok(Self { info, bytes })
    }
}
impl ReadTarget {
    #[must_use]
    pub fn byte_count(self) -> usize {
        self.bytes
    }
}

pub struct DirectReadback {
    api: GlesApi,
    enabled: bool,
    slots: Vec<Slot>,
    free: VecDeque<usize>,
    pending: VecDeque<usize>,
    byte_count: usize,
}
impl DirectReadback {
    /// # Errors
    /// Returns an unavailable GLES provider or required entrypoint.
    pub fn new() -> Result<Self> {
        Ok(Self::with_api(GlesApi::open("libGLESv2.so")?))
    }
    #[must_use]
    pub fn with_api(api: GlesApi) -> Self {
        let enabled = std::env::var("CLUSTER_ASYNC_NV12_READBACK").as_deref() != Ok("0")
            && api.async_available();
        Self {
            api,
            enabled,
            slots: Vec::new(),
            free: VecDeque::new(),
            pending: VecDeque::new(),
            byte_count: 0,
        }
    }
    /// # Errors
    /// Returns provider errors only on enabled TICI hosts.
    pub fn create_tici() -> Result<Option<Self>> {
        if std::env::var("CLUSTER_DIRECT_NV12_READBACK").as_deref() == Ok("0")
            || !Path::new("/TICI").is_file()
        {
            return Ok(None);
        }
        Self::new().map(Some)
    }
    #[must_use]
    pub fn async_supported(&self) -> bool {
        self.enabled
    }
    #[must_use]
    pub fn async_can_enqueue(&self) -> bool {
        self.enabled && (self.slots.is_empty() || !self.free.is_empty())
    }
    /// # Errors
    /// Rejects invalid target/destination metadata or a GLES readback failure.
    pub fn read_rgba(&mut self, info: TargetInfo, destination: &mut [u8]) -> Result<()> {
        let target = ReadTarget::try_from(info)?;
        let info = target.info;
        if destination.len() < target.bytes {
            return Err(Error::Destination {
                actual: destination.len(),
                required: target.bytes,
                asynchronous: false,
            });
        }
        let framebuffer = binding(self.api.integer(GL_FRAMEBUFFER_BINDING));
        let alignment = self.api.integer(GL_PACK_ALIGNMENT);
        let buffer = binding(self.api.integer(GL_PIXEL_PACK_BUFFER_BINDING));
        self.clear_errors("direct NV12 readback")?;
        let result = (|| -> Result<()> {
            self.api.bind_framebuffer(GL_FRAMEBUFFER, info.framebuffer);
            self.api.bind_buffer(GL_PIXEL_PACK_BUFFER, 0);
            self.api.pixel_store(GL_PACK_ALIGNMENT, 4);
            self.api.read_rgba((info.width, info.height), destination)?;
            let error = self.api.error();
            if error != 0 {
                return Err(Error::Gl {
                    operation: "direct NV12 readback",
                    code: error,
                });
            }
            Ok(())
        })();
        self.api.pixel_store(GL_PACK_ALIGNMENT, alignment);
        self.api.bind_buffer(GL_PIXEL_PACK_BUFFER, buffer);
        self.api.bind_framebuffer(GL_FRAMEBUFFER, framebuffer);
        result
    }
    /// # Errors
    /// Returns an unavailable resource deletion entrypoint.
    pub fn disable_async(&mut self) -> Result<()> {
        self.close()?;
        self.enabled = false;
        Ok(())
    }
    /// # Errors
    /// Returns an unavailable resource deletion entrypoint.
    pub fn close(&mut self) -> Result<()> {
        for slot in &mut self.slots {
            if let Some(fence) = slot.fence.take() {
                self.api.delete_fence(fence)?;
            }
        }
        if !self.slots.is_empty() {
            self.api.delete_buffers(
                &self
                    .slots
                    .iter()
                    .map(|slot| slot.buffer)
                    .collect::<Vec<_>>(),
            )?;
        }
        self.slots.clear();
        self.free.clear();
        self.pending.clear();
        self.byte_count = 0;
        Ok(())
    }
    pub(super) fn clear_errors(&self, operation: &'static str) -> Result<()> {
        for _ in 0..16 {
            if self.api.error() == 0 {
                return Ok(());
            }
        }
        Err(Error::Uncleared(operation))
    }
}
impl Drop for DirectReadback {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
