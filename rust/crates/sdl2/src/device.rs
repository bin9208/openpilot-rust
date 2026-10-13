use crate::{Error, Inner};
use std::{
    ffi::{c_void, CStr},
    ptr::NonNull,
    rc::Rc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceKind {
    Controller,
    Joystick,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub kind: DeviceKind,
    pub axes: Vec<i16>,
    pub buttons: Vec<bool>,
    pub hat: u8,
}

pub struct Device {
    inner: Rc<Inner>,
    handle: Option<NonNull<c_void>>,
    kind: DeviceKind,
}
impl Device {
    pub(crate) fn open(inner: Rc<Inner>, index: i32) -> Result<Self, Error> {
        // SAFETY: SDL checks the device index and returns null on failure.
        if unsafe { (inner.api.is_controller)(index) } != 0 {
            // SAFETY: the index refers to an SDL controller; null remains checked.
            if let Some(handle) = NonNull::new(unsafe { (inner.api.controller_open)(index) }) {
                return Ok(Self {
                    inner,
                    handle: Some(handle),
                    kind: DeviceKind::Controller,
                });
            }
        }
        // SAFETY: SDL validates the index; null is converted to an error.
        let handle = NonNull::new(unsafe { (inner.api.joystick_open)(index) })
            .ok_or_else(|| inner.error())?;
        Ok(Self {
            inner,
            handle: Some(handle),
            kind: DeviceKind::Joystick,
        })
    }
    /// # Errors
    /// Returns an error if the device has already been closed.
    pub fn name(&self) -> Result<String, Error> {
        let handle = self.handle.ok_or(Error::Closed)?.as_ptr();
        // SAFETY: the handle is open and the accessor matches its kind.
        let name = unsafe {
            match self.kind {
                DeviceKind::Controller => (self.inner.api.controller_name)(handle),
                DeviceKind::Joystick => (self.inner.api.joystick_name)(handle),
            }
        };
        if name.is_null() {
            return Err(self.inner.error());
        }
        // SAFETY: SDL owns this NUL-terminated name while the handle stays open.
        Ok(unsafe { CStr::from_ptr(name) }
            .to_string_lossy()
            .into_owned())
    }
    /// # Errors
    /// Returns an error if the device has already been closed.
    pub fn snapshot(&mut self) -> Result<Snapshot, Error> {
        let handle = self.handle.ok_or(Error::Closed)?.as_ptr();
        let api = &self.inner.api;
        // SAFETY: all calls use the live handle's matching API on its owning thread.
        unsafe {
            (api.pump_events)();
            (api.joystick_update)();
            let (axes, buttons, hat) = match self.kind {
                DeviceKind::Controller => (
                    (0..6).map(|i| (api.controller_axis)(handle, i)).collect(),
                    (0..15)
                        .map(|i| (api.controller_button)(handle, i) != 0)
                        .collect(),
                    0,
                ),
                DeviceKind::Joystick => (
                    (0..(api.joystick_axes)(handle).clamp(0, 6))
                        .map(|i| (api.joystick_axis)(handle, i))
                        .collect(),
                    (0..(api.joystick_buttons)(handle).clamp(0, 15))
                        .map(|i| (api.joystick_button)(handle, i) != 0)
                        .collect(),
                    if (api.joystick_hats)(handle) > 0 {
                        (api.joystick_hat)(handle, 0)
                    } else {
                        0
                    },
                ),
            };
            Ok(Snapshot {
                kind: self.kind,
                axes,
                buttons,
                hat,
            })
        }
    }
    pub fn close(&mut self) {
        if let Some(handle) = self.handle.take() {
            // SAFETY: this handle is owned, open, and closed exactly once by its matching API.
            unsafe {
                match self.kind {
                    DeviceKind::Controller => (self.inner.api.controller_close)(handle.as_ptr()),
                    DeviceKind::Joystick => (self.inner.api.joystick_close)(handle.as_ptr()),
                }
            };
        }
    }
}
impl Drop for Device {
    fn drop(&mut self) {
        self.close();
    }
}
