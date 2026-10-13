mod api;
mod device;
#[cfg(feature = "gui")]
pub mod gui;
#[cfg(feature = "fixture-inspection")]
mod virtual_device;

pub use device::{Device, DeviceKind, Snapshot};
use std::{ffi::CStr, path::Path, rc::Rc};
#[cfg(feature = "fixture-inspection")]
pub use virtual_device::VirtualDevice;

const SUBSYSTEMS: u32 = 0x200 | 0x2000 | 0x4000;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("SDL2 library: {0}")]
    Library(#[from] libloading::Error),
    #[error("SDL2: {0}")]
    Sdl(String),
    #[error("SDL2 device is closed")]
    Closed,
}

pub(crate) struct Inner {
    api: api::Api,
}
impl Inner {
    fn error(&self) -> Error {
        // SAFETY: SDL_GetError returns a thread-local, NUL-terminated string.
        Error::Sdl(
            unsafe { CStr::from_ptr((self.api.get_error)()) }
                .to_string_lossy()
                .into_owned(),
        )
    }
}
impl Drop for Inner {
    fn drop(&mut self) {
        // SAFETY: this balances the successful initialization owned by this context.
        unsafe { (self.api.quit_subsystem)(SUBSYSTEMS) };
    }
}

#[derive(Clone)]
pub struct Context {
    inner: Rc<Inner>,
}
impl Context {
    /// # Errors
    /// Returns an error when the library cannot load or SDL cannot initialize.
    pub fn load(path: &Path) -> Result<Self, Error> {
        let api = api::Api::load(path)?;
        // SAFETY: the loaded signature matches SDL2 and flags identify input subsystems.
        if unsafe { (api.init_subsystem)(SUBSYSTEMS) } != 0 {
            // SAFETY: SDL_GetError is valid after a failed initialization.
            return Err(Error::Sdl(
                unsafe { CStr::from_ptr((api.get_error)()) }
                    .to_string_lossy()
                    .into_owned(),
            ));
        }
        Ok(Self {
            inner: Rc::new(Inner { api }),
        })
    }
    #[must_use]
    pub fn device_count(&self) -> i32 {
        // SAFETY: this context holds an initialized joystick subsystem.
        unsafe { (self.inner.api.num_joysticks)() }
    }
    /// # Errors
    /// Returns SDL's error if the selected device cannot be opened.
    pub fn open(&self, index: i32) -> Result<Device, Error> {
        Device::open(Rc::clone(&self.inner), index)
    }
}
