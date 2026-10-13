use super::{abi::Descriptor, api::Api, Devices};
use crate::Error;
use std::{ptr, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct DeviceIdentity {
    pub vendor: u16,
    pub product: u16,
    pub bus: u8,
    pub address: u8,
}

/// Descriptor-only discovery context: no handle is opened by `find`.
pub struct Scanner {
    api: Rc<Api>,
    context: super::abi::Pointer,
}

impl Scanner {
    /// # Errors
    /// Returns library, context, enumeration or device-descriptor failures.
    pub fn new() -> Result<Self, Error> {
        let mut scanner = Self {
            api: Api::system()?,
            context: ptr::null_mut(),
        };
        // SAFETY: category8; live writable storage receives a context owned by Scanner, including errors.
        let code = unsafe { (scanner.api.init)(&mut scanner.context) };
        scanner.api.checked(code, "libusb_init")?;
        if scanner.context.is_null() {
            return Err(Error::Contract("libusb_init returned null context"));
        }
        Ok(scanner)
    }

    /// Returns the first matching descriptor, independently of open permission.
    /// # Errors
    /// Returns enumeration or descriptor failures without opening any device.
    pub fn find(&self, vendor: u16, product: u16) -> Result<Option<DeviceIdentity>, Error> {
        let mut devices = Devices {
            api: Rc::clone(&self.api),
            pointer: ptr::null_mut(),
        };
        // SAFETY: category8; the live context returns a referenced list retained by Devices.
        let count = unsafe { (self.api.list)(self.context, &mut devices.pointer) };
        let count = usize::try_from(count).map_err(|_| Error::UsbApi {
            operation: "libusb_get_device_list",
            code: i32::try_from(count).unwrap_or(-99),
            message: self.api.text(i32::try_from(count).unwrap_or(-99)),
        })?;
        if count > 0 && devices.pointer.is_null() {
            return Err(Error::Contract("libusb returned null device list"));
        }
        for position in 0..count {
            // SAFETY: category10; position lies in the SDK's live count-element list.
            let device = unsafe { *devices.pointer.add(position) };
            if device.is_null() {
                return Err(Error::Contract("libusb returned null listed device"));
            }
            let mut descriptor = Descriptor::default();
            // SAFETY: category8; the list retains device and Descriptor has the checked SDK layout.
            let code = unsafe { (self.api.descriptor)(device, &mut descriptor) };
            self.api.checked(code, "libusb_get_device_descriptor")?;
            if (descriptor.vendor, descriptor.product) == (vendor, product) {
                // SAFETY: category3; both immutable synchronous queries borrow the device retained by list.
                let bus = unsafe { (self.api.bus)(device) };
                // SAFETY: category3; the device remains retained until this synchronous call completes.
                let address = unsafe { (self.api.address)(device) };
                return Ok(Some(DeviceIdentity {
                    vendor,
                    product,
                    bus,
                    address,
                }));
            }
        }
        Ok(None)
    }
}

impl Drop for Scanner {
    fn drop(&mut self) {
        if !self.context.is_null() {
            // SAFETY: category12; every per-call list was freed before the sole context exits.
            unsafe { (self.api.exit)(self.context) };
        }
    }
}
