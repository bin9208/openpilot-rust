use crate::{bridge::ffi, Error};
use std::{marker::PhantomData, rc::Rc};

pub struct GlesApi {
    api: cxx::UniquePtr<ffi::GlesApi>,
    _thread: PhantomData<Rc<()>>,
}
pub struct ReadMap<'a> {
    mapping: cxx::UniquePtr<ffi::GlMapping>,
    _api: &'a mut GlesApi,
}
impl GlesApi {
    /// # Errors
    /// Returns a library load or required GLES entrypoint failure.
    pub fn open(path: &str) -> Result<Self, Error> {
        Ok(Self {
            api: ffi::gles_api(path)?,
            _thread: PhantomData,
        })
    }
    #[must_use]
    pub fn integer(&self, parameter: u32) -> i32 {
        self.api.integer(parameter)
    }
    #[must_use]
    pub fn error(&self) -> u32 {
        self.api.gl_error()
    }
    pub fn bind_framebuffer(&mut self, target: u32, name: u32) {
        self.api.bind_framebuffer(target, name);
    }
    pub fn bind_buffer(&mut self, target: u32, name: u32) {
        self.api.bind_buffer(target, name);
    }
    pub fn pixel_store(&mut self, parameter: u32, value: i32) {
        self.api.pixel_store(parameter, value);
    }
    /// # Errors
    /// Rejects an undersized destination, pack-buffer binding, or invalid GL pack state.
    pub fn read_rgba(&mut self, size: (i32, i32), destination: &mut [u8]) -> Result<(), Error> {
        Ok(self.api.read_rgba(size.0, size.1, destination)?)
    }
    /// # Errors
    /// Rejects absent or undersized pack-buffer storage and invalid pack state.
    pub fn read_rgba_pbo(&mut self, size: (i32, i32)) -> Result<(), Error> {
        Ok(self.api.read_rgba_pbo(size.0, size.1)?)
    }
    #[must_use]
    pub fn async_available(&self) -> bool {
        self.api.async_available()
    }
    /// # Errors
    /// Returns a required DMA target or fence entrypoint that is unavailable.
    pub fn prepare_dma(&self) -> Result<(), Error> {
        Ok(self.api.prepare_dma()?)
    }
    /// # Errors
    /// Returns a missing buffer entrypoint or an invalid native element count.
    pub fn generate_buffers(&mut self, count: u32) -> Result<Vec<u32>, Error> {
        Ok(self.api.generate_buffers(count)?)
    }
    /// # Errors
    /// Returns a missing deletion entrypoint or an invalid native element count.
    pub fn delete_buffers(&mut self, names: &[u32]) -> Result<(), Error> {
        Ok(self.api.delete_buffers(names)?)
    }
    /// # Errors
    /// Rejects a size beyond the native address range or a missing allocation entrypoint.
    pub fn allocate_buffer(&mut self, target: u32, size: u64, usage: u32) -> Result<(), Error> {
        Ok(self.api.allocate_buffer(target, size, usage)?)
    }
    /// # Errors
    /// Returns a missing GL synchronization entrypoint.
    pub fn fence(&mut self, condition: u32, flags: u32) -> Result<u64, Error> {
        Ok(self.api.fence(condition, flags)?)
    }
    /// # Errors
    /// Returns a missing GL synchronization entrypoint.
    pub fn wait_fence(&mut self, fence: u64, flags: u32, timeout: u64) -> Result<u32, Error> {
        Ok(self.api.wait_fence(fence, flags, timeout)?)
    }
    /// # Errors
    /// Returns a missing GL synchronization entrypoint.
    pub fn delete_fence(&mut self, fence: u64) -> Result<(), Error> {
        Ok(self.api.delete_fence(fence)?)
    }
    /// # Errors
    /// Returns a missing GL flush entrypoint.
    pub fn flush(&mut self) -> Result<(), Error> {
        Ok(self.api.flush()?)
    }
    /// # Errors
    /// Rejects absent, undersized, unmappable or invalid buffer storage.
    /// ```compile_fail,E0499
    /// use openpilot_startup_ui::gles::GlesApi;
    /// let mut api = GlesApi::open("libGLESv2.so").unwrap();
    /// let mapping = api.map_read(0x88EB, 16).unwrap();
    /// api.bind_buffer(0x88EB, 0);
    /// mapping.copy_to(&mut [0; 16]).unwrap();
    /// ```
    pub fn map_read(&mut self, target: u32, size: u64) -> Result<ReadMap<'_>, Error> {
        let mapping = self.api.map_read(target, size)?;
        Ok(ReadMap {
            mapping,
            _api: self,
        })
    }
    /// # Errors
    /// Returns a missing texture allocation entrypoint.
    pub fn generate_texture(&mut self) -> Result<u32, Error> {
        Ok(self.api.generate_texture()?)
    }
    /// # Errors
    /// Returns a missing texture deletion entrypoint.
    pub fn delete_texture(&mut self, texture: u32) -> Result<(), Error> {
        Ok(self.api.delete_texture(texture)?)
    }
    /// # Errors
    /// Returns a missing texture binding entrypoint.
    pub fn bind_texture(&mut self, target: u32, texture: u32) -> Result<(), Error> {
        Ok(self.api.bind_texture(target, texture)?)
    }
    /// # Errors
    /// Returns a missing texture parameter entrypoint.
    pub fn texture_parameter(
        &mut self,
        target: u32,
        parameter: u32,
        value: i32,
    ) -> Result<(), Error> {
        Ok(self.api.texture_parameter(target, parameter, value)?)
    }
    /// # Errors
    /// Returns a missing framebuffer allocation entrypoint.
    pub fn generate_framebuffer(&mut self) -> Result<u32, Error> {
        Ok(self.api.generate_framebuffer()?)
    }
    /// # Errors
    /// Returns a missing framebuffer deletion entrypoint.
    pub fn delete_framebuffer(&mut self, framebuffer: u32) -> Result<(), Error> {
        Ok(self.api.delete_framebuffer(framebuffer)?)
    }
    /// # Errors
    /// Returns a missing framebuffer attachment entrypoint.
    pub fn attach_texture(
        &mut self,
        target: u32,
        attachment: u32,
        texture: u32,
    ) -> Result<(), Error> {
        Ok(self.api.attach_texture(target, attachment, texture)?)
    }
    /// # Errors
    /// Returns a missing framebuffer status entrypoint.
    pub fn framebuffer_status(&self, target: u32) -> Result<u32, Error> {
        Ok(self.api.framebuffer_status(target)?)
    }
}
impl ReadMap<'_> {
    /// # Errors
    /// Rejects an inactive mapping or an undersized destination.
    pub fn copy_to(&self, destination: &mut [u8]) -> Result<(), Error> {
        Ok(self.mapping.copy_to(destination)?)
    }
    #[must_use]
    pub fn unmap(mut self) -> bool {
        self.mapping.pin_mut().unmap()
    }
}
