use crate::{Context, DeviceKind, Error, Inner};
use std::{
    ffi::{c_int, c_void},
    ptr::NonNull,
    rc::Rc,
};

type Attach = unsafe extern "C" fn(c_int, c_int, c_int, c_int) -> c_int;
type Detach = unsafe extern "C" fn(c_int) -> c_int;
type InstanceId = unsafe extern "C" fn(*mut c_void) -> c_int;
type DeviceId = unsafe extern "C" fn(c_int) -> c_int;
type Axis = unsafe extern "C" fn(*mut c_void, c_int, i16) -> c_int;
type Button = unsafe extern "C" fn(*mut c_void, c_int, u8) -> c_int;

pub struct VirtualDevice {
    inner: Rc<Inner>,
    handle: NonNull<c_void>,
    instance: i32,
    detach: Detach,
    device_id: DeviceId,
    set_axis: Axis,
    set_button: Button,
    set_hat: Button,
}
impl Context {
    /// # Errors
    /// Returns an error if SDL lacks virtual devices or cannot create one.
    pub fn attach_virtual(
        &self,
        kind: DeviceKind,
        axes: i32,
        buttons: i32,
        hats: i32,
    ) -> Result<VirtualDevice, Error> {
        let library = &self.inner.api._library;
        // SAFETY: these SDL2 virtual-device signatures match the library retained by inner.
        let (attach, detach, instance_id, device_id, set_axis, set_button, set_hat) = unsafe {
            (
                *library.get::<Attach>(b"SDL_JoystickAttachVirtual\0")?,
                *library.get::<Detach>(b"SDL_JoystickDetachVirtual\0")?,
                *library.get::<InstanceId>(b"SDL_JoystickInstanceID\0")?,
                *library.get::<DeviceId>(b"SDL_JoystickGetDeviceInstanceID\0")?,
                *library.get::<Axis>(b"SDL_JoystickSetVirtualAxis\0")?,
                *library.get::<Button>(b"SDL_JoystickSetVirtualButton\0")?,
                *library.get::<Button>(b"SDL_JoystickSetVirtualHat\0")?,
            )
        };
        // SAFETY: SDL validates the counts; device type 1 is GAMECONTROLLER and 0 is UNKNOWN.
        let index = unsafe {
            attach(
                i32::from(kind == DeviceKind::Controller),
                axes,
                buttons,
                hats,
            )
        };
        if index < 0 {
            return Err(self.inner.error());
        }
        // SAFETY: index was just returned by SDL; the handle is checked before use.
        let Some(handle) = NonNull::new(unsafe { (self.inner.api.joystick_open)(index) }) else {
            let error = self.inner.error();
            // SAFETY: detach only the virtual device this function created.
            unsafe { detach(index) };
            return Err(error);
        };
        // SAFETY: handle is a live SDL joystick.
        let instance = unsafe { instance_id(handle.as_ptr()) };
        Ok(VirtualDevice {
            inner: Rc::clone(&self.inner),
            handle,
            instance,
            detach,
            device_id,
            set_axis,
            set_button,
            set_hat,
        })
    }
}
impl VirtualDevice {
    /// # Errors
    /// Returns an error if this owned device is no longer attached.
    pub fn device_index(&self) -> Result<i32, Error> {
        // SAFETY: SDL remains initialized; indices are checked by SDL during this same-thread query.
        (0..unsafe { (self.inner.api.num_joysticks)() })
            .find(|index| unsafe { (self.device_id)(*index) } == self.instance)
            .ok_or_else(|| Error::Sdl("virtual device detached".into()))
    }
    fn checked(&self, result: i32) -> Result<(), Error> {
        if result == 0 {
            Ok(())
        } else {
            Err(self.inner.error())
        }
    }
    /// # Errors
    /// Returns SDL's error for an invalid axis index.
    pub fn set_axis(&self, axis: i32, value: i16) -> Result<(), Error> {
        // SAFETY: the live handle is owned and SDL validates the axis index.
        self.checked(unsafe { (self.set_axis)(self.handle.as_ptr(), axis, value) })
    }
    /// # Errors
    /// Returns SDL's error for an invalid button index.
    pub fn set_button(&self, button: i32, pressed: bool) -> Result<(), Error> {
        // SAFETY: the live handle is owned and SDL validates the button index.
        self.checked(unsafe { (self.set_button)(self.handle.as_ptr(), button, u8::from(pressed)) })
    }
    /// # Errors
    /// Returns SDL's error for an invalid hat index or value.
    pub fn set_hat(&self, hat: i32, value: u8) -> Result<(), Error> {
        // SAFETY: the live handle is owned and SDL validates the hat index and value.
        self.checked(unsafe { (self.set_hat)(self.handle.as_ptr(), hat, value) })
    }
}
impl Drop for VirtualDevice {
    fn drop(&mut self) {
        let index = self.device_index();
        // SAFETY: close this owned handle exactly once before detaching its device.
        unsafe { (self.inner.api.joystick_close)(self.handle.as_ptr()) };
        if let Ok(index) = index {
            // SAFETY: the instance-id lookup prevents detaching a different device after index renumbering.
            unsafe { (self.detach)(index) };
        }
    }
}
