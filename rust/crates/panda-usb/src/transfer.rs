use crate::{connection::Connection, Error, Log, Session};
use std::{
    ffi::CStr,
    sync::{atomic::Ordering, MutexGuard},
};

const NO_DEVICE: i32 = -4;
const TIMEOUT: i32 = -7;
const OVERFLOW: i32 = -8;

impl Session {
    pub fn connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }
    pub fn healthy(&self) -> bool {
        self.healthy.load(Ordering::SeqCst)
    }
    pub fn disconnect(&self) {
        self.connected.store(false, Ordering::SeqCst);
    }

    fn lock(&self) -> Result<MutexGuard<'_, Connection>, Error> {
        self.connection.lock().map_err(|_| Error::Poisoned)
    }

    fn issue(&self, connection: &Connection, code: i32, operation: &'static str) {
        // SAFETY: libusb returns a static nul-terminated description for every code.
        let description = unsafe { CStr::from_ptr((connection.api.strerror)(code)) }
            .to_string_lossy()
            .into_owned();
        (self.log)(Log::Issue {
            code,
            description,
            operation,
        });
        if code == NO_DEVICE {
            (self.log)(Log::Disconnected);
            self.disconnect();
        }
    }

    pub fn control_write(
        &self,
        request: u8,
        value: u16,
        index: u16,
        timeout: u32,
    ) -> Result<i32, Error> {
        self.control(request, value, index, &mut [], timeout, false)
    }

    pub fn control_read(
        &self,
        request: u8,
        value: u16,
        index: u16,
        data: &mut [u8],
        timeout: u32,
    ) -> Result<i32, Error> {
        self.control(request, value, index, data, timeout, true)
    }

    fn control(
        &self,
        request: u8,
        value: u16,
        index: u16,
        data: &mut [u8],
        timeout: u32,
        read: bool,
    ) -> Result<i32, Error> {
        if !self.connected() {
            return Ok(NO_DEVICE);
        }
        let length = u16::try_from(data.len())
            .map_err(|_| Error::Contract("control buffer exceeds uint16 length"))?;
        let connection = self.lock()?;
        loop {
            // SAFETY: the mutex owns the live handle. The mutable buffer is exclusive
            // through the synchronous call; zero-length output uses the source's null.
            let result = unsafe {
                (connection.api.control)(
                    connection.handle,
                    if read { 0xc0 } else { 0x40 },
                    request,
                    value,
                    index,
                    if read {
                        data.as_mut_ptr()
                    } else {
                        std::ptr::null_mut()
                    },
                    length,
                    timeout,
                )
            };
            if result < 0 {
                self.issue(
                    &connection,
                    result,
                    if read {
                        "control_read"
                    } else {
                        "control_write"
                    },
                );
            }
            if result >= 0 || !self.connected() {
                return Ok(result);
            }
        }
    }

    pub fn bulk_write(&self, endpoint: u8, data: &mut [u8], timeout: u32) -> Result<i32, Error> {
        self.bulk(endpoint, data, timeout, false)
    }

    pub fn bulk_read(&self, endpoint: u8, data: &mut [u8], timeout: u32) -> Result<i32, Error> {
        self.bulk(endpoint, data, timeout, true)
    }

    fn bulk(&self, endpoint: u8, data: &mut [u8], timeout: u32, read: bool) -> Result<i32, Error> {
        if !self.connected() {
            return Ok(0);
        }
        let length = i32::try_from(data.len())
            .map_err(|_| Error::Contract("bulk buffer exceeds int length"))?;
        let connection = self.lock()?;
        let mut transferred = 0;
        loop {
            // SAFETY: synchronous libusb accepts this exclusive buffer for the
            // checked length; transferred is a live out-parameter, retained on retries.
            let result = unsafe {
                (connection.api.bulk)(
                    connection.handle,
                    endpoint,
                    data.as_mut_ptr(),
                    length,
                    &mut transferred,
                    timeout,
                )
            };
            if result == TIMEOUT {
                if !read {
                    (self.log)(Log::TransmitFull);
                }
                return Ok(transferred);
            }
            if read && result == OVERFLOW {
                self.healthy.store(false, Ordering::SeqCst);
                (self.log)(Log::Overflow(transferred));
            } else if result != 0 || (!read && length != transferred) {
                self.issue(
                    &connection,
                    result,
                    if read { "bulk_read" } else { "bulk_write" },
                );
            }
            if result == 0 || !self.connected() {
                return Ok(transferred);
            }
        }
    }
}
