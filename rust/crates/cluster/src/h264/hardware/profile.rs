use super::Hardware;
use crate::renderer::nv12::Nv12Layout;
use std::{io, os::fd::BorrowedFd, time::Instant};
#[derive(Clone, Copy, Debug)]
pub enum InputStage {
    Dma,
    Pbo,
    Direct,
}
impl InputStage {
    const fn names(self) -> [&'static str; 3] {
        match self {
            Self::Dma => [
                "main.usb_h264.acquire_dmabuf",
                "main.usb.render_nv12_dmabuf_total",
                "main.usb_h264.submit_dmabuf",
            ],
            Self::Pbo => [
                "main.usb_h264.acquire_pbo",
                "main.usb_h264.copy_pbo",
                "main.usb_h264.submit_pbo",
            ],
            Self::Direct => [
                "main.usb_h264.acquire_direct",
                "main.usb.render_nv12_total",
                "main.usb_h264.submit_direct",
            ],
        }
    }
}
impl Hardware {
    /// # Errors
    /// Preserves the mapped input lifecycle while measuring the selected source output stages.
    pub fn render_input_profiled(
        &mut self,
        stage: InputStage,
        draw: impl FnOnce(&mut [u8], BorrowedFd<'_>, Nv12Layout) -> io::Result<()>,
    ) -> io::Result<()> {
        self.render_input_stages(matches!(stage, InputStage::Dma), Some(stage.names()), draw)
    }
    pub(super) fn profile_stage(&self, name: Option<&str>, started: Instant) -> io::Result<()> {
        if let (Some(name), Some(monitor)) = (name, &self.packets.monitor) {
            monitor.elapsed(name, started)?;
        }
        Ok(())
    }
}
