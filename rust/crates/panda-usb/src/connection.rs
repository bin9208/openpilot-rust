use crate::{
    api::{Api, Descriptor, Pointer},
    Error, Log, Logger,
};
use std::{
    ptr,
    sync::{atomic::AtomicBool, Arc, Mutex},
};

pub(crate) struct Connection {
    pub api: Arc<Api>,
    pub context: Pointer,
    pub handle: Pointer,
}

// SAFETY: libusb contexts and synchronous handles can move between threads.
// Session serializes every operation on this connection with its mutex.
unsafe impl Send for Connection {}

impl Drop for Connection {
    fn drop(&mut self) {
        // SAFETY: this owner exclusively releases its handle before its context.
        // Failed construction retains the same release/close order as the source.
        unsafe {
            if !self.handle.is_null() {
                (self.api.release)(self.handle, 0);
                (self.api.close)(self.handle);
            }
            if !self.context.is_null() {
                (self.api.exit)(self.context);
            }
        }
    }
}

pub(crate) struct DeviceList {
    pub api: Arc<Api>,
    pub pointer: *mut Pointer,
}
impl Drop for DeviceList {
    fn drop(&mut self) {
        if !self.pointer.is_null() {
            // SAFETY: the list came from this API and still owns its device refs.
            unsafe { (self.api.free_list)(self.pointer, 1) };
        }
    }
}

pub struct Session {
    pub(crate) connection: Mutex<Connection>,
    pub(crate) connected: AtomicBool,
    pub(crate) healthy: AtomicBool,
    pub(crate) log: Logger,
    serial: Vec<u8>,
}

impl Session {
    pub fn open(api: Arc<Api>, serial: &[u8], log: Logger) -> Result<Self, Error> {
        let mut connection = Connection {
            api,
            context: ptr::null_mut(),
            handle: ptr::null_mut(),
        };
        // SAFETY: initialization writes one owned context pointer. The descriptor,
        // list and 26-byte serial buffers remain live throughout synchronous calls.
        let hardware_serial = unsafe {
            if (connection.api.initialize)(&mut connection.context) != 0 {
                connection.context = ptr::null_mut();
                log(Log::Initialization);
                return Err(Error::Connection);
            }
            if connection.context.is_null() {
                return Err(Error::Contract(
                    "successful initialization returned null context",
                ));
            }
            (connection.api.option)(connection.context, 0, 3_i32);
            let mut list = DeviceList {
                api: Arc::clone(&connection.api),
                pointer: ptr::null_mut(),
            };
            let count = (connection.api.list)(connection.context, &mut list.pointer);
            if count < 0 {
                return Err(Error::Connection);
            }
            if count > 0 && list.pointer.is_null() {
                return Err(Error::Contract("nonempty device list is null"));
            }
            let mut selected = Vec::new();
            for index in 0..count as usize {
                let device = *list.pointer.add(index);
                let mut descriptor = Descriptor::default();
                if (connection.api.descriptor)(device, &mut descriptor) < 0 {
                    return Err(Error::Connection);
                }
                if descriptor.vendor != 0x3801 || descriptor.product != 0xddcc {
                    continue;
                }
                let ret = (connection.api.open)(device, &mut connection.handle);
                if connection.handle.is_null() || ret < 0 {
                    return Err(Error::Connection);
                }
                let mut buffer = [0_u8; 26];
                let count = (connection.api.serial)(
                    connection.handle,
                    descriptor.serial,
                    buffer.as_mut_ptr(),
                    26,
                );
                if count < 0 {
                    return Err(Error::Connection);
                }
                let value = buffer
                    .get(..count as usize)
                    .ok_or(Error::Contract("serial exceeds requested buffer"))?;
                selected = value.to_vec();
                if serial.is_empty() || serial == value {
                    break;
                }
                (connection.api.close)(connection.handle);
                connection.handle = ptr::null_mut();
            }
            if connection.handle.is_null() {
                return Err(Error::Connection);
            }
            drop(list);
            if (connection.api.active)(connection.handle, 0) == 1 {
                (connection.api.detach)(connection.handle, 0);
            }
            if (connection.api.configure)(connection.handle, 1) != 0
                || (connection.api.claim)(connection.handle, 0) != 0
            {
                return Err(Error::Connection);
            }
            selected
        };
        Ok(Self {
            connection: Mutex::new(connection),
            connected: AtomicBool::new(true),
            healthy: AtomicBool::new(true),
            log,
            serial: hardware_serial,
        })
    }

    pub fn serial(&self) -> &[u8] {
        &self.serial
    }
}
