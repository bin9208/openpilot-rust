use crate::{bridge::ffi, EncodeError, Layout, Options};
use std::{marker::PhantomData, os::unix::ffi::OsStrExt, path::Path, rc::Rc};
/// A thread-confined owned external TurboJPEG library; each call owns a compressor.
/// ```compile_fail
/// fn send<T: Send>() {}
/// send::<openpilot_jpeg::TurboJpeg>();
/// ```
pub struct TurboJpeg {
    inner: cxx::UniquePtr<ffi::Turbo>,
    thread: PhantomData<Rc<()>>,
}
impl TurboJpeg {
    /// # Errors
    /// Returns provider loading, missing ABI symbols.
    pub fn open(library: &Path) -> Result<Self, cxx::Exception> {
        Ok(Self {
            inner: ffi::open_turbo(library.as_os_str().as_bytes())?,
            thread: PhantomData,
        })
    }
    /// # Errors
    /// Returns a packed-pixel contract or external compressor initialization/encoding failure.
    pub fn encode(
        &mut self,
        pixels: &[u8],
        layout: Layout,
        options: Options,
    ) -> Result<Vec<u8>, EncodeError> {
        layout.check_pixels(pixels).map_err(EncodeError::Contract)?;
        ffi::encode_turbo(
            self.inner.pin_mut(),
            pixels,
            ffi::Layout {
                width: layout.width(),
                height: layout.height(),
                components: layout.color().components(),
            },
            options.quality().value(),
        )
        .map_err(EncodeError::Native)
    }
}
