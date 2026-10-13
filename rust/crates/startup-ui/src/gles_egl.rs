use crate::{bridge::ffi, Error};
use std::rc::Rc;

struct Api {
    raw: cxx::UniquePtr<ffi::EglApi>,
    display: u64,
}
pub struct EglImageApi {
    inner: Rc<Api>,
}
pub struct ImportedEglImage {
    api: Rc<Api>,
    image: u64,
}
impl EglImageApi {
    /// # Errors
    /// Returns an unavailable provider, required entrypoint or current EGL display.
    pub fn current(egl: &str, gles: &str) -> Result<Self, Error> {
        let raw = ffi::egl_image_api(egl, gles)?;
        let display = raw.current_display();
        if display == 0 {
            return Err(Error::Contract("EGL has no current display"));
        }
        Ok(Self {
            inner: Rc::new(Api { raw, display }),
        })
    }
    #[must_use]
    pub fn extensions(&self) -> String {
        self.inner.raw.extensions(self.inner.display)
    }
    #[must_use]
    pub fn error(&self) -> i32 {
        self.inner.raw.error()
    }
    #[must_use]
    pub fn prepare_images(&self) -> bool {
        self.inner.raw.prepare_images()
    }
    /// # Errors
    /// Rejects malformed terminated attributes; EGL import failure returns None.
    pub fn import(&self, attributes: &[i32]) -> Result<Option<ImportedEglImage>, Error> {
        let image = self
            .inner
            .raw
            .create_image(self.inner.display, attributes)?;
        Ok((image != 0).then(|| ImportedEglImage {
            api: Rc::clone(&self.inner),
            image,
        }))
    }
}
impl ImportedEglImage {
    /// # Errors
    /// Returns an unavailable EGL image binding function.
    pub fn bind_target(&self, target: u32) -> Result<(), Error> {
        Ok(self.api.raw.image_target(target, self.image)?)
    }
}
impl Drop for ImportedEglImage {
    fn drop(&mut self) {
        self.api.raw.destroy_image(self.api.display, self.image);
    }
}
