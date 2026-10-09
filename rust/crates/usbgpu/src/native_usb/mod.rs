//! Owned libusb context/handle. Rc prevents moving event handling to another thread.
mod abi;
mod api;
mod batch;
mod sync;

use crate::{transport::Description, Error};
use abi::{Descriptor, Pointer};
use api::Api;
use std::{ptr, rc::Rc};

pub struct Usb {
    api: Rc<Api>,
    context: Pointer,
    handle: Pointer,
    claimed: bool,
    endpoints: Vec<u8>,
}
struct Devices {
    api: Rc<Api>,
    pointer: *mut Pointer,
}
impl Drop for Devices {
    fn drop(&mut self) {
        if !self.pointer.is_null() {
            // SAFETY: this list is released exactly once while its API and context are still alive.
            unsafe { (self.api.free_list)(self.pointer, 1) };
        }
    }
}
impl Usb {
    pub fn open(vendor: u16, product: u16, mut index: u32) -> Result<Option<Self>, Error> {
        let api = Api::system()?;
        let mut usb = Self {
            api,
            context: ptr::null_mut(),
            handle: ptr::null_mut(),
            claimed: false,
            endpoints: Vec::new(),
        };
        // SAFETY: init writes to live pointer storage; Usb owns even an error-path returned context.
        let code = unsafe { (usb.api.init)(&mut usb.context) };
        usb.api.checked(code, "libusb_init")?;
        if usb.context.is_null() {
            return Err(Error::Contract("libusb_init returned null context"));
        }
        let mut devices = Devices {
            api: Rc::clone(&usb.api),
            pointer: ptr::null_mut(),
        };
        // SAFETY: the live context creates a referenced list retained through descriptor/open calls below.
        let count = unsafe { (usb.api.list)(usb.context, &mut devices.pointer) };
        let count = usize::try_from(count).map_err(|_| Error::UsbApi {
            operation: "libusb_get_device_list",
            code: i32::try_from(count).unwrap_or(-99),
            message: usb.api.text(i32::try_from(count).unwrap_or(-99)),
        })?;
        if count > 0 && devices.pointer.is_null() {
            return Err(Error::Contract("libusb returned null device list"));
        }
        for position in 0..count {
            // SAFETY: position is inside the live libusb list with exactly count device pointers.
            let device = unsafe { *devices.pointer.add(position) };
            if device.is_null() {
                return Err(Error::Contract("libusb returned null listed device"));
            }
            let mut descriptor = Descriptor::default();
            // SAFETY: Descriptor matches the SDK layout and the list retains the live device.
            let code = unsafe { (usb.api.descriptor)(device, &mut descriptor) };
            usb.api.checked(code, "libusb_get_device_descriptor")?;
            if (descriptor.vendor, descriptor.product) != (vendor, product) {
                continue;
            }
            if index > 0 {
                index -= 1;
                continue;
            }
            // SAFETY: open writes one handle owned by Usb, which outlives its referenced device list.
            let code = unsafe { (usb.api.open)(device, &mut usb.handle) };
            usb.api.checked(code, "libusb_open")?;
            if usb.handle.is_null() {
                return Err(Error::Contract("libusb_open returned null handle"));
            }
            return Ok(Some(usb));
        }
        Ok(None)
    }

    fn description(&self) -> Result<Description, Error> {
        // SAFETY: get_device borrows the referenced device from this live handle.
        let device = unsafe { (self.api.device)(self.handle) };
        if device.is_null() {
            return Err(Error::Contract("libusb returned null handle device"));
        }
        let mut descriptor = Descriptor::default();
        // SAFETY: live device plus writable checked-layout descriptor; neither pointer is retained.
        let code = unsafe { (self.api.descriptor)(device, &mut descriptor) };
        self.api.checked(code, "libusb_get_device_descriptor")?;
        let mut bytes = [0; 256];
        // SAFETY: the exclusively writable output buffer stays live for this synchronous libusb call.
        let length = unsafe {
            (self.api.string)(
                self.handle,
                descriptor.product_string,
                bytes.as_mut_ptr(),
                256,
            )
        };
        self.api
            .checked(length, "libusb_get_string_descriptor_ascii")?;
        let length = usize::try_from(length)
            .map_err(|_| Error::Contract("negative product descriptor length"))?;
        let product = bytes
            .get(..length)
            .ok_or(Error::Contract("libusb product descriptor exceeds buffer"))?
            .to_vec();
        // SAFETY: these synchronous immutable queries borrow the same live device held by this handle.
        let bus = unsafe { (self.api.bus)(device) };
        // SAFETY: the referenced device remains live through this synchronous address query.
        let address = unsafe { (self.api.address)(device) };
        Ok(Description {
            bus,
            address,
            product,
        })
    }
}
impl Drop for Usb {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            if !self.endpoints.is_empty() {
                // SAFETY: batch has drained every terminal callback; endpoints belong to this live handle.
                let code = unsafe {
                    (self.api.free_streams)(
                        self.handle,
                        self.endpoints.as_mut_ptr(),
                        i32::try_from(self.endpoints.len()).unwrap_or(32),
                    )
                };
                if code < 0 {
                    eprintln!(
                        "usbgpu: release USB streams failed: {}",
                        self.api.text(code)
                    );
                }
            }
            if self.claimed {
                // SAFETY: interface0 was claimed on this handle and no transfer remains outstanding.
                let code = unsafe { (self.api.release)(self.handle, 0) };
                if code < 0 {
                    eprintln!(
                        "usbgpu: release USB interface failed: {}",
                        self.api.text(code)
                    );
                }
            }
            // SAFETY: the exclusively owned handle is closed once after stream/interface/transfer release.
            unsafe { (self.api.close)(self.handle) };
        }
        if !self.context.is_null() {
            // SAFETY: all handles/lists/transfers have gone; this owner exits its one context before unloading its API.
            unsafe { (self.api.exit)(self.context) };
        }
    }
}
