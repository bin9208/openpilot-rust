mod error;
mod layout;
mod target;
mod wait;

pub use error::{Error, Result};
pub use layout::PackedLayout;
use openpilot_startup_ui::{
    gles::GlesApi,
    gles_egl::{EglImageApi, ImportedEglImage},
    renderer::{Renderer, TargetInfo},
};
use std::{
    collections::BTreeMap,
    os::fd::{AsRawFd, BorrowedFd, OwnedFd},
    path::Path,
};

pub struct DmaTargetPool {
    egl: EglImageApi,
    gl: GlesApi,
    targets: BTreeMap<(i32, usize, usize), ImportedTarget>,
}
struct ImportedTarget {
    info: TargetInfo,
    _image: ImportedEglImage,
    _fd: OwnedFd,
}
pub struct ImportedTargetView<'a> {
    target: &'a ImportedTarget,
}

impl DmaTargetPool {
    /// # Errors
    /// Returns unavailable current EGL display, DMA import extension or provider functions.
    pub fn new() -> Result<Self> {
        Self::with_providers("libEGL.so", "libGLESv2.so")
    }
    /// # Errors
    /// Returns unavailable current EGL display, DMA import extension or provider functions.
    pub fn with_providers(egl_path: &str, gles_path: &str) -> Result<Self> {
        let egl = EglImageApi::current(egl_path, gles_path).map_err(|error| match error {
            openpilot_startup_ui::Error::Contract("EGL has no current display") => Error::Display,
            error => Error::Native(error),
        })?;
        let gl = GlesApi::open(gles_path)?;
        gl.prepare_dma()?;
        if !egl.extensions().contains("EGL_EXT_image_dma_buf_import") {
            return Err(Error::Extension);
        }
        if !egl.prepare_images() {
            return Err(Error::Functions);
        }
        Ok(Self {
            egl,
            gl,
            targets: BTreeMap::new(),
        })
    }
    /// # Errors
    /// Returns provider errors only when DMA output is enabled on a TICI host.
    pub fn create_tici() -> Result<Option<Self>> {
        if std::env::var("CLUSTER_NV12_DMABUF_OUTPUT").as_deref() == Ok("0")
            || !Path::new("/TICI").is_file()
        {
            return Ok(None);
        }
        Self::new().map(Some)
    }
    /// # Errors
    /// Rejects invalid packed layout, FD duplication, EGL import or framebuffer allocation.
    /// ```compile_fail,E0499
    /// use openpilot_cluster::renderer::nv12::dmabuf::DmaTargetPool;
    /// use std::{fs::File, os::fd::AsFd};
    /// let file = File::open("owned-buffer").unwrap();
    /// let mut pool = DmaTargetPool::new().unwrap();
    /// let target = pool.target_for(file.as_fd(), 8, 16).unwrap();
    /// pool.close().unwrap();
    /// target.info();
    /// ```
    pub fn target_for(
        &mut self,
        fd: BorrowedFd<'_>,
        stride: usize,
        bytes: usize,
    ) -> Result<ImportedTargetView<'_>> {
        let layout = PackedLayout::new(stride, bytes)?;
        let key = (fd.as_raw_fd(), stride, bytes);
        if !self.targets.contains_key(&key) {
            let target = self.import_target(fd, layout)?;
            self.targets.insert(key, target);
        }
        let target = self.targets.get(&key).ok_or(Error::Layout)?;
        Ok(ImportedTargetView { target })
    }
    /// # Errors
    /// Returns an unavailable native resource deletion function.
    pub fn close(&mut self) -> Result<()> {
        for (_, target) in std::mem::take(&mut self.targets) {
            self.gl.delete_framebuffer(target.info.framebuffer)?;
            self.gl.delete_texture(target.info.texture)?;
            drop(target);
        }
        Ok(())
    }
    fn clear_errors(&self, operation: &'static str) -> Result<()> {
        for _ in 0..16 {
            if self.gl.error() == 0 {
                return Ok(());
            }
        }
        Err(Error::Uncleared(operation))
    }
}
impl Drop for DmaTargetPool {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            eprintln!("cluster DMA-BUF cleanup: {error}");
        }
    }
}
impl ImportedTargetView<'_> {
    #[must_use]
    pub fn info(&self) -> TargetInfo {
        self.target.info
    }
    /// # Errors
    /// Returns native target metadata or callback drawing errors; always ends texture mode.
    pub fn draw<T>(
        &self,
        renderer: &mut Renderer,
        draw: impl FnOnce(&mut Renderer) -> std::result::Result<T, openpilot_startup_ui::Error>,
    ) -> std::result::Result<T, openpilot_startup_ui::Error> {
        renderer.with_external_target(self.target.info, draw)
    }
}
