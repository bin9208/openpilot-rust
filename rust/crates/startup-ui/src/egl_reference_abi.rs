//! Raw, validated ABI calls for an independent reference policy. No EGL lifecycle policy.
use crate::bridge::ffi;
pub use ffi::{ReferenceCore as Core, ReferenceExtension as Extension};
pub struct RawApi {
    inner: cxx::UniquePtr<ffi::ReferenceApi>,
}
impl Default for RawApi {
    fn default() -> Self {
        Self::new()
    }
}
impl RawApi {
    pub fn new() -> Self {
        Self {
            inner: ffi::reference_api(),
        }
    }
    pub fn load_egl(&mut self) -> Result<(), cxx::Exception> {
        self.inner.pin_mut().load_egl()
    }
    pub fn load_gles(&mut self) -> Result<(), cxx::Exception> {
        self.inner.pin_mut().load_gles()
    }
    pub fn resolve_core(&mut self, symbol: Core) -> Result<(), cxx::Exception> {
        self.inner.pin_mut().resolve_core(symbol)
    }
    pub fn resolve_proc(&mut self, symbol: Extension) -> Result<bool, cxx::Exception> {
        self.inner.pin_mut().resolve_proc(symbol)
    }
    pub fn resolve_library(&mut self, symbol: Extension) -> Result<(), cxx::Exception> {
        self.inner.pin_mut().resolve_library(symbol)
    }
    pub fn display(&mut self) -> Result<u64, cxx::Exception> {
        self.inner.pin_mut().reference_display()
    }
    pub fn initialize(&self, display: u64) -> Result<bool, cxx::Exception> {
        self.inner.reference_initialize(display)
    }
    pub fn extensions(&self, display: u64) -> Result<Vec<u8>, cxx::Exception> {
        self.inner.reference_extensions(display)
    }
    pub fn error(&self) -> Result<i32, cxx::Exception> {
        self.inner.reference_error()
    }
    pub fn create(
        &mut self,
        display: u64,
        target: u32,
        attrs: &[i32],
    ) -> Result<u64, cxx::Exception> {
        self.inner
            .pin_mut()
            .reference_create(display, target, attrs)
    }
    pub fn active(&self, texture: u32) -> Result<(), cxx::Exception> {
        self.inner.reference_active(texture)
    }
    pub fn bind(&self, target: u32, texture: u32) -> Result<(), cxx::Exception> {
        self.inner.reference_bind(target, texture)
    }
    pub fn image_target(&self, target: u32, image: u64) -> Result<(), cxx::Exception> {
        self.inner.reference_image_target(target, image)
    }
    pub fn destroy(&mut self, display: u64, image: u64) -> Result<bool, cxx::Exception> {
        self.inner.pin_mut().reference_destroy(display, image)
    }
}

/// Consume descriptor ownership and close once, preserving recoverable errno
/// when a foreign EGL implementation has already closed the transferred FD.
pub fn close_descriptor(fd: std::os::fd::OwnedFd) -> std::io::Result<()> {
    use std::os::fd::IntoRawFd;
    let error = ffi::reference_close_owned_descriptor(fd.into_raw_fd());
    if error == 0 {
        Ok(())
    } else {
        Err(std::io::Error::from_raw_os_error(error))
    }
}
