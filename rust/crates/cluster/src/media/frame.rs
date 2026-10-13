use openpilot_startup_ui::egl::FrameLayout;
use std::{
    io,
    os::fd::{AsFd, BorrowedFd, OwnedFd},
    sync::{mpsc::Sender, Arc},
};

#[derive(Debug)]
pub struct HardwareFrame {
    pub layout: FrameLayout,
    pub index: u32,
    pub generation: u64,
    sequence: u64,
    fd: Option<OwnedFd>,
    release: Sender<u32>,
    failures: Option<Sender<String>>,
}

pub struct FrameLease {
    pub index: u32,
    pub generation: u64,
    pub sequence: u64,
    pub release: Sender<u32>,
    pub failures: Sender<String>,
}

impl HardwareFrame {
    /// # Errors
    /// Rejects dimensions/stride/UV offsets that do not describe an NV12 buffer.
    pub fn new(
        fd: OwnedFd,
        layout: FrameLayout,
        index: u32,
        release: Sender<u32>,
    ) -> io::Result<Arc<Self>> {
        Self::create(fd, layout, index, release, None, 0, 0)
    }
    /// # Errors
    /// Rejects a decoded NV12 layout before acquiring its generation lease.
    pub fn retained(fd: OwnedFd, layout: FrameLayout, lease: FrameLease) -> io::Result<Arc<Self>> {
        Self::create(
            fd,
            layout,
            lease.index,
            lease.release,
            Some(lease.failures),
            lease.generation,
            lease.sequence,
        )
    }
    fn create(
        fd: OwnedFd,
        layout: FrameLayout,
        index: u32,
        release: Sender<u32>,
        failures: Option<Sender<String>>,
        generation: u64,
        sequence: u64,
    ) -> io::Result<Arc<Self>> {
        let minimum_uv = layout.stride.checked_mul(layout.height);
        if layout.width <= 0
            || layout.height <= 0
            || layout.width % 2 != 0
            || layout.height % 2 != 0
            || layout.stride < layout.width
            || minimum_uv.is_none_or(|offset| offset > layout.uv_offset)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid decoded NV12 frame layout",
            ));
        }
        Ok(Arc::new(Self {
            layout,
            index,
            fd: Some(fd),
            release,
            generation,
            sequence,
            failures,
        }))
    }
    #[must_use]
    pub fn token(&self) -> (u64, u32) {
        (self.generation, self.index)
    }
    #[must_use]
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn mark_egl_import_failed(&self, reason: &str) {
        if let Some(failures) = &self.failures {
            let _ = failures.send(
                if reason.is_empty() {
                    "TICI H264 EGL import failed"
                } else {
                    reason
                }
                .into(),
            );
        }
    }
}
impl AsFd for HardwareFrame {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_ref().expect("live hardware frame FD").as_fd()
    }
}
impl Drop for HardwareFrame {
    fn drop(&mut self) {
        drop(self.fd.take());
        let _ = self.release.send(self.index);
    }
}

pub(crate) mod optional {
    use super::HardwareFrame;
    use serde::{ser::Error, Deserialize, Deserializer, Serializer};
    use std::sync::Arc;
    #[expect(
        clippy::ref_option,
        reason = "serde with calls this function with a field reference"
    )]
    pub fn serialize<S: Serializer>(
        value: &Option<Arc<HardwareFrame>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        if value.is_some() {
            return Err(S::Error::custom(
                "hardware frame cannot be serialized as a host fixture",
            ));
        }
        serializer.serialize_none()
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Arc<HardwareFrame>>, D::Error> {
        Option::<()>::deserialize(deserializer)?;
        Ok(None)
    }
}
