//! EGL NV12 image ownership and recovery policy from system/ui/lib/egl.py (MIT).
use crate::{bridge::ffi, Error};
use std::{
    cell::Cell,
    marker::PhantomData,
    os::fd::{AsRawFd, BorrowedFd, OwnedFd},
    rc::Rc,
};
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct FrameLayout {
    pub width: i32,
    pub height: i32,
    pub stride: i32,
    pub uv_offset: i32,
}
impl FrameLayout {
    pub fn attributes(self, fd: i32) -> [i32; 19] {
        [
            0x3057,
            self.width,
            0x3056,
            self.height,
            0x3271,
            842_094_158,
            0x3272,
            fd,
            0x3273,
            0,
            0x3274,
            self.stride,
            0x3275,
            fd,
            0x3276,
            self.uv_offset,
            0x3277,
            self.stride,
            0x3038,
        ]
    }
}
#[derive(Debug, thiserror::Error)]
pub enum EglError {
    #[error(transparent)]
    Native(#[from] Error),
    #[error("EGL has no current display")]
    NoDisplay,
    #[error("EGL initialize: {0}")]
    Initialize(String),
    #[error("EGL create image: {0}")]
    Create(String),
}
pub struct Context {
    api: cxx::UniquePtr<ffi::EglApi>,
    display: Cell<u64>,
    _thread: PhantomData<Rc<()>>,
}
impl Context {
    pub fn new() -> Result<Self, EglError> {
        Self::with_libraries("libEGL.so", "libGLESv2.so")
    }
    pub fn with_libraries(egl: &str, gles: &str) -> Result<Self, EglError> {
        let api = ffi::egl_api(egl, gles).map_err(Error::from)?;
        let context = Self {
            api,
            display: Cell::new(0),
            _thread: PhantomData,
        };
        context.initialize(true)?;
        Ok(context)
    }
    fn initialize(&self, force: bool) -> Result<(), EglError> {
        let display = self.api.current_display();
        if display == 0 {
            return Err(EglError::NoDisplay);
        }
        if !force && self.display.get() == display {
            return Ok(());
        }
        if !self.api.initialize(display) {
            return Err(EglError::Initialize(error_text(self.api.error())));
        }
        self.display.set(display);
        if !self
            .api
            .extensions(display)
            .contains("EGL_EXT_image_dma_buf_import")
        {
            eprintln!("Current EGL display does not advertise EGL_EXT_image_dma_buf_import");
        }
        Ok(())
    }
    pub fn create(&self, layout: FrameLayout, fd: BorrowedFd<'_>) -> Result<Image<'_>, EglError> {
        let fd = rustix::io::dup(fd)
            .map_err(std::io::Error::from)
            .map_err(Error::from)?;
        let attributes = layout.attributes(fd.as_raw_fd());
        for attempt in 0..2 {
            let image = self
                .api
                .create_image(self.display.get(), &attributes)
                .map_err(Error::from)?;
            if image != 0 {
                return Ok(Image {
                    context: self,
                    display: self.display.get(),
                    image,
                    fd,
                });
            }
            let error = self.api.error();
            if error == 0x3001 && attempt == 0 && self.initialize(true).is_ok() {
                continue;
            }
            return Err(EglError::Create(error_text(error)));
        }
        Err(EglError::Create("retry exhausted".into()))
    }
}
pub struct Image<'context> {
    context: &'context Context,
    display: u64,
    image: u64,
    fd: OwnedFd,
}
impl Image<'_> {
    pub fn bind(&self, texture: u32) {
        self.context.api.bind_image(texture, self.image);
    }
    pub fn duplicated_fd(&self) -> BorrowedFd<'_> {
        use std::os::fd::AsFd;
        self.fd.as_fd()
    }
}
impl Drop for Image<'_> {
    fn drop(&mut self) {
        if !self.context.api.destroy_image(self.display, self.image) {
            eprintln!(
                "Failed to destroy EGL image: {}",
                error_text(self.context.api.error())
            );
        }
    }
}
pub fn error_text(error: i32) -> String {
    let name = match error {
        0x3000 => "EGL_SUCCESS",
        0x3001 => "EGL_NOT_INITIALIZED",
        0x3002 => "EGL_BAD_ACCESS",
        0x3003 => "EGL_BAD_ALLOC",
        0x3004 => "EGL_BAD_ATTRIBUTE",
        0x3005 => "EGL_BAD_CONFIG",
        0x3006 => "EGL_BAD_CONTEXT",
        0x3007 => "EGL_BAD_CURRENT_SURFACE",
        0x3008 => "EGL_BAD_DISPLAY",
        0x3009 => "EGL_BAD_MATCH",
        0x300a => "EGL_BAD_NATIVE_PIXMAP",
        0x300b => "EGL_BAD_NATIVE_WINDOW",
        0x300c => "EGL_BAD_PARAMETER",
        0x300d => "EGL_BAD_SURFACE",
        0x300e => "EGL_CONTEXT_LOST",
        _ => "EGL_UNKNOWN_ERROR",
    };
    format!("0x{error:04X} {name}")
}
