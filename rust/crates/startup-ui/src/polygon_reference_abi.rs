//! Safe ownership wrapper for separate raw raylib calls, without scene policy.
use crate::bridge::ffi;
use std::{os::unix::ffi::OsStrExt, path::Path};
pub struct RawApi {
    inner: cxx::UniquePtr<ffi::PolygonReferenceApi>,
}
impl RawApi {
    pub fn new(library: &Path) -> Result<Self, cxx::Exception> {
        Ok(Self {
            inner: ffi::polygon_reference_api(library.as_os_str().as_bytes())?,
        })
    }
    pub fn flags(&mut self, flags: u32) -> Result<(), cxx::Exception> {
        self.inner.pin_mut().polygon_flags(flags)
    }
    pub fn initialize(
        &mut self,
        width: i32,
        height: i32,
        title: &str,
    ) -> Result<(), cxx::Exception> {
        self.inner
            .pin_mut()
            .polygon_initialize(width, height, title)
    }
    pub fn fps(&mut self, fps: i32) -> Result<(), cxx::Exception> {
        self.inner.pin_mut().polygon_fps(fps)
    }
    pub fn begin(&mut self) -> Result<(), cxx::Exception> {
        self.inner.pin_mut().polygon_begin()
    }
    pub fn clear(&mut self, color: u32) -> Result<(), cxx::Exception> {
        self.inner.pin_mut().polygon_clear(color)
    }
    pub fn load_shader(&mut self, vertex: &str, fragment: &str) -> Result<(), cxx::Exception> {
        self.inner.pin_mut().polygon_load_shader(vertex, fragment)
    }
    pub fn location(&mut self, name: &str) -> Result<(), cxx::Exception> {
        self.inner.pin_mut().polygon_location(name)
    }
    pub fn integer(&mut self, name: &str, value: i32) -> Result<(), cxx::Exception> {
        self.inner.pin_mut().polygon_integer(name, value)
    }
    pub fn floats(
        &mut self,
        name: &str,
        values: &[f32],
        components: u32,
        vector: bool,
    ) -> Result<(), cxx::Exception> {
        self.inner
            .pin_mut()
            .polygon_floats(name, values, components, vector)
    }
    pub fn matrix(&mut self, name: &str, values: &[f32]) -> Result<(), cxx::Exception> {
        self.inner.pin_mut().polygon_matrix(name, values)
    }
    pub fn begin_shader(&mut self) -> Result<(), cxx::Exception> {
        self.inner.pin_mut().polygon_begin_shader()
    }
    pub fn strip(&mut self, points: &[f32], color: u32) -> Result<(), cxx::Exception> {
        self.inner.pin_mut().polygon_strip(points, color)
    }
    pub fn end_shader(&mut self) -> Result<(), cxx::Exception> {
        self.inner.pin_mut().polygon_end_shader()
    }
    pub fn flush(&mut self) -> Result<(), cxx::Exception> {
        self.inner.pin_mut().polygon_flush()
    }
    pub fn load_capture(&mut self) -> Result<u64, cxx::Exception> {
        self.inner.pin_mut().polygon_load_capture()
    }
    pub fn export_capture(&mut self, image: u64, path: &Path) -> Result<bool, cxx::Exception> {
        self.inner
            .pin_mut()
            .polygon_export_capture(image, path.as_os_str().as_bytes())
    }
    pub fn unload_capture(&mut self, image: u64) -> Result<(), cxx::Exception> {
        self.inner.pin_mut().polygon_unload_capture(image)
    }
    pub fn end(&mut self) -> Result<(), cxx::Exception> {
        self.inner.pin_mut().polygon_end()
    }
    pub fn unload_shader(&mut self) -> Result<(), cxx::Exception> {
        self.inner.pin_mut().polygon_unload_shader()
    }
    pub fn close(&mut self) -> Result<(), cxx::Exception> {
        self.inner.pin_mut().polygon_close()
    }
}
