use crate::{
    api::Descriptor,
    connection::{Connection, DeviceList},
    Api, Error, Log, Logger,
};
use std::{
    ptr,
    sync::{Arc, Mutex},
};

pub struct Enumerator {
    connection: Option<Mutex<Connection>>,
    log: Logger,
}

impl Enumerator {
    pub fn new(api: Arc<Api>, log: Logger) -> Result<Self, Error> {
        let mut connection = Connection {
            api,
            context: ptr::null_mut(),
            handle: ptr::null_mut(),
        };
        // SAFETY: initialization writes this owner's context; it outlives each list.
        let code = unsafe { (connection.api.initialize)(&mut connection.context) };
        if code != 0 {
            connection.context = ptr::null_mut();
            log(Log::Initialization);
            return Ok(Self {
                connection: None,
                log,
            });
        }
        if connection.context.is_null() {
            return Err(Error::Contract(
                "successful initialization returned null context",
            ));
        }
        // SAFETY: option 0 with an int argument is libusb's log-level option.
        unsafe { (connection.api.option)(connection.context, 0, 3_i32) };
        Ok(Self {
            connection: Some(Mutex::new(connection)),
            log,
        })
    }

    pub fn list(&self) -> Result<Vec<Vec<u8>>, Error> {
        let Some(connection) = &self.connection else {
            return Ok(Vec::new());
        };
        let mut connection = connection.lock().map_err(|_| Error::Poisoned)?;
        let mut devices = DeviceList {
            api: Arc::clone(&connection.api),
            pointer: ptr::null_mut(),
        };
        // SAFETY: the context remains live while this list owns its device refs.
        let count = unsafe { (connection.api.list)(connection.context, &mut devices.pointer) };
        if count < 0 {
            (self.log)(Log::DeviceList);
            return Ok(Vec::new());
        }
        if count > 0 && devices.pointer.is_null() {
            return Err(Error::Contract("nonempty device list is null"));
        }
        let mut serials = Vec::new();
        for index in 0..count as usize {
            // SAFETY: each device pointer is within the list's count. Descriptor and
            // serial storage live through each synchronous call; close follows read.
            unsafe {
                let device = *devices.pointer.add(index);
                let mut descriptor = Descriptor::default();
                if (connection.api.descriptor)(device, &mut descriptor) < 0 {
                    break;
                }
                if descriptor.vendor != 0x3801 || descriptor.product != 0xddcc {
                    continue;
                }
                let ret = (connection.api.open)(device, &mut connection.handle);
                if ret < 0 {
                    break;
                }
                if connection.handle.is_null() {
                    return Err(Error::Contract("successful open returned null handle"));
                }
                let mut buffer = [0; 26];
                let size = (connection.api.serial)(
                    connection.handle,
                    descriptor.serial,
                    buffer.as_mut_ptr(),
                    26,
                );
                (connection.api.close)(connection.handle);
                connection.handle = ptr::null_mut();
                if size < 0 {
                    break;
                }
                let serial = buffer
                    .get(..size as usize)
                    .ok_or(Error::Contract("serial exceeds requested buffer"))?;
                serials.push(serial.to_vec());
            }
        }
        Ok(serials)
    }
}
