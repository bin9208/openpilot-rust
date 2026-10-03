use crate::{
    api::{Descriptor, Pointer},
    connection::DeviceList,
    Api, Error,
};
use std::{ptr, rc::Rc, sync::Arc};

fn checked(code: i32, operation: &'static str) -> Result<usize, Error> {
    if code < 0 {
        Err(Error::Usb { operation, code })
    } else {
        Ok(code as usize)
    }
}

struct Owner {
    api: Arc<Api>,
    pointer: Pointer,
}
impl Drop for Owner {
    fn drop(&mut self) {
        // SAFETY: every list and handle retains this owner; the final drop exclusively releases its live context.
        unsafe { (self.api.exit)(self.pointer) };
    }
}

pub struct Context {
    owner: Rc<Owner>,
}
impl Context {
    pub fn new(api: Arc<Api>) -> Result<Self, Error> {
        let mut pointer = ptr::null_mut();
        // SAFETY: libusb writes one context pointer to live storage and takes no reference to that storage.
        checked(unsafe { (api.initialize)(&mut pointer) }, "initialization")?;
        if pointer.is_null() {
            return Err(Error::Contract(
                "successful initialization returned null context",
            ));
        }
        Ok(Self {
            owner: Rc::new(Owner { api, pointer }),
        })
    }

    pub fn devices(&self) -> Result<Devices, Error> {
        let mut list = DeviceList {
            api: self.owner.api.clone(),
            pointer: ptr::null_mut(),
        };
        // SAFETY: the live context creates a ref-counted list which Devices releases before dropping its owner.
        let count = unsafe { (self.owner.api.list)(self.owner.pointer, &mut list.pointer) };
        if count < 0 {
            return Err(Error::Usb {
                operation: "device list",
                code: i32::try_from(count).unwrap_or(-99),
            });
        }
        if count > 0 && list.pointer.is_null() {
            return Err(Error::Contract("nonempty device list is null"));
        }
        Ok(Devices {
            list,
            owner: self.owner.clone(),
            count: count as usize,
        })
    }
}

pub struct Devices {
    list: DeviceList,
    owner: Rc<Owner>,
    count: usize,
}
impl Devices {
    pub fn len(&self) -> usize {
        self.count
    }
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    pub fn get(&self, index: usize) -> Option<Device<'_>> {
        if index >= self.count {
            return None;
        }
        // SAFETY: index is within the live libusb list; the returned borrow prevents releasing that list.
        let pointer = unsafe { *self.list.pointer.add(index) };
        (!pointer.is_null()).then_some(Device {
            devices: self,
            pointer,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DeviceDescriptor {
    pub vendor: u16,
    pub product: u16,
    pub bcd: u16,
    pub serial_index: u8,
}
pub struct Device<'a> {
    devices: &'a Devices,
    pointer: Pointer,
}
impl Device<'_> {
    pub fn descriptor(&self) -> Result<DeviceDescriptor, Error> {
        let mut descriptor = Descriptor::default();
        // SAFETY: the list retains this device; Descriptor has the checked libusb C layout and lives through the call.
        checked(
            unsafe { (self.devices.owner.api.descriptor)(self.pointer, &mut descriptor) },
            "descriptor",
        )?;
        Ok(DeviceDescriptor {
            vendor: descriptor.vendor,
            product: descriptor.product,
            bcd: descriptor.device,
            serial_index: descriptor.serial,
        })
    }

    pub fn open(&self) -> Result<Handle, Error> {
        let mut pointer = ptr::null_mut();
        // SAFETY: opening a live listed device creates a handle which retains the device independently of the list.
        checked(
            unsafe { (self.devices.owner.api.open)(self.pointer, &mut pointer) },
            "open",
        )?;
        if pointer.is_null() {
            return Err(Error::Contract("successful open returned null handle"));
        }
        Ok(Handle {
            owner: self.devices.owner.clone(),
            pointer,
        })
    }
}

pub struct Handle {
    owner: Rc<Owner>,
    pointer: Pointer,
}
impl Handle {
    fn live(&self) -> Result<Pointer, Error> {
        if self.pointer.is_null() {
            Err(Error::Contract("USB handle is closed"))
        } else {
            Ok(self.pointer)
        }
    }

    pub fn close(&mut self) {
        if !self.pointer.is_null() {
            // SAFETY: this handle has one owner, excludes concurrent access through Rc, and is invalidated immediately.
            unsafe { (self.owner.api.close)(self.pointer) };
            self.pointer = ptr::null_mut();
        }
    }

    pub fn auto_detach(&mut self, enabled: bool) -> Result<(), Error> {
        let handle = self.live()?;
        let function = self.owner.api.auto_detach()?;
        // SAFETY: the function belongs to the retained API and receives a live handle plus libusb's integer boolean.
        checked(
            unsafe { function(handle, i32::from(enabled)) },
            "auto detach",
        )
        .map(|_| ())
    }

    pub fn claim(&mut self, interface: i32) -> Result<(), Error> {
        let handle = self.live()?;
        // SAFETY: synchronous interface claim operates on this exclusively borrowed live handle.
        checked(
            unsafe { (self.owner.api.claim)(handle, interface) },
            "claim",
        )
        .map(|_| ())
    }

    pub fn ascii_string(&mut self, index: u8) -> Result<Option<String>, Error> {
        if index == 0 {
            return Ok(None);
        }
        let handle = self.live()?;
        let mut data = [0_u8; 255];
        // SAFETY: this writable 255-byte array and live handle remain valid for the synchronous descriptor call.
        let code = unsafe { (self.owner.api.serial)(handle, index, data.as_mut_ptr(), 255) };
        if code == -5 {
            return Ok(None);
        }
        let length = checked(code, "ASCII string")?;
        let bytes = data
            .get(..length)
            .ok_or(Error::Contract("ASCII descriptor exceeds requested length"))?;
        if !bytes.is_ascii() {
            return Err(Error::Contract("USB serial descriptor is not ASCII"));
        }
        Ok(Some(String::from_utf8(bytes.to_vec()).map_err(|_| {
            Error::Contract("USB serial descriptor is not ASCII")
        })?))
    }

    pub fn string(&mut self, index: u8, language: u16) -> Result<Option<String>, Error> {
        if index == 0 {
            return Ok(None);
        }
        let data = match self.control_read(0x80, 6, 0x300 | u16::from(index), language, 255, 1000) {
            Err(Error::Usb { code: -5, .. }) => return Ok(None),
            result => result?,
        };
        if data.len() < 2 || data[1] != 3 {
            return Err(Error::Contract("invalid USB string descriptor"));
        }
        let end = data.len().min(usize::from(data[0])).max(2);
        let bytes = &data[2..end];
        if bytes.len() % 2 != 0 {
            return Err(Error::Contract("odd UTF-16 USB string length"));
        }
        let words: Vec<_> = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        String::from_utf16(&words)
            .map(Some)
            .map_err(|_| Error::Contract("invalid UTF-16 USB string"))
    }

    pub fn control_read(
        &mut self,
        kind: u8,
        request: u8,
        value: u16,
        index: u16,
        length: usize,
        timeout: u32,
    ) -> Result<Vec<u8>, Error> {
        let length = u16::try_from(length)
            .map_err(|_| Error::Contract("USB control read exceeds uint16 length"))?;
        let mut data = vec![0; usize::from(length)];
        let handle = self.live()?;
        // SAFETY: the writable buffer has exactly length bytes; libusb retains neither pointer after this synchronous call.
        let result = unsafe {
            (self.owner.api.control)(
                handle,
                kind | 0x80,
                request,
                value,
                index,
                data.as_mut_ptr(),
                length,
                timeout,
            )
        };
        let count = checked(result, "control read")?;
        if count > data.len() {
            return Err(Error::Contract("USB control read exceeds requested length"));
        }
        data.truncate(count);
        Ok(data)
    }

    pub fn control_write(
        &mut self,
        kind: u8,
        request: u8,
        value: u16,
        index: u16,
        data: &[u8],
        timeout: u32,
    ) -> Result<usize, Error> {
        let length = u16::try_from(data.len())
            .map_err(|_| Error::Contract("USB control write exceeds uint16 length"))?;
        let mut data = data.to_vec();
        let handle = self.live()?;
        // SAFETY: OUT direction is enforced and the owned writable buffer stays live through the synchronous call.
        let result = unsafe {
            (self.owner.api.control)(
                handle,
                kind & 0x7f,
                request,
                value,
                index,
                data.as_mut_ptr(),
                length,
                timeout,
            )
        };
        checked(result, "control write")
    }

    pub fn bulk_write(&mut self, endpoint: u8, data: &[u8], timeout: u32) -> Result<usize, Error> {
        let length = i32::try_from(data.len())
            .map_err(|_| Error::Contract("USB bulk write exceeds int length"))?;
        let mut data = data.to_vec();
        let mut transferred = 0_i32;
        let handle = self.live()?;
        // SAFETY: the OUT buffer and transferred count are live writable storage, and the live handle owns its context.
        let result = unsafe {
            (self.owner.api.bulk)(
                handle,
                endpoint & 0x7f,
                data.as_mut_ptr(),
                length,
                &mut transferred,
                timeout,
            )
        };
        checked(result, "bulk write")?;
        usize::try_from(transferred)
            .map_err(|_| Error::Contract("negative USB bulk transfer length"))
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        self.close();
    }
}
