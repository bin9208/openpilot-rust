use super::rtmp_api::{AVal, Api};
use crate::Error;
use std::{
    ffi::c_void,
    ptr::{self, NonNull},
    sync::Arc,
};

pub(super) struct Handle {
    api: Arc<Api>,
    pointer: NonNull<c_void>,
    url: Vec<u8>,
    tc_url: Vec<u8>,
}
// SAFETY: An opaque RTMP instance has no owner-thread affinity (the original
// calls it on executor threads). Client's mutex exclusively serializes every
// call and destruction. No Sync implementation or raw pointer is exposed.
#[expect(
    unsafe_code,
    reason = "Move a uniquely owned librtmp instance only behind Client's mutex"
)]
unsafe impl Send for Handle {}
impl Handle {
    #[expect(
        unsafe_code,
        reason = "Create, initialize and configure one checked opaque librtmp instance"
    )]
    pub fn new(api: Arc<Api>, url: &str, tc_url: &str) -> Result<Self, Error> {
        let url = terminated(url)?;
        let tc_bytes = terminated(tc_url)?;
        // SAFETY: Alloc returns either null or an owned RTMP allocation. It is
        // checked before Init; Handle Drop frees it on all subsequent errors.
        let pointer = unsafe { NonNull::new((api.alloc)()) }
            .ok_or_else(|| Error::Source("librtmp allocation failed".into()))?;
        let mut handle = Self {
            api,
            pointer,
            url,
            tc_url: tc_bytes,
        };
        // SAFETY: The initialized instance is exclusively borrowed. SetupURL
        // mutates/stores pointers into url; both buffers stay allocated through
        // Close and Free, and are never resized after these calls.
        unsafe {
            (handle.api.init)(handle.pointer.as_ptr());
            if (handle.api.setup)(handle.pointer.as_ptr(), handle.url.as_mut_ptr().cast()) == 0 {
                return Err(Error::Source("librtmp URL setup failed".into()));
            }
            if !tc_url.is_empty() {
                let mut name = b"tcUrl\0".to_vec();
                let option = AVal {
                    value: name.as_mut_ptr().cast(),
                    length: 5,
                };
                let argument = AVal {
                    value: handle.tc_url.as_mut_ptr().cast(),
                    length: i32::try_from(handle.tc_url.len() - 1)
                        .map_err(|_| Error::Source("librtmp option failed: tcUrl".into()))?,
                };
                if (handle.api.option)(handle.pointer.as_ptr(), &option, &argument) == 0 {
                    return Err(Error::Source("librtmp option failed: tcUrl".into()));
                }
            }
            (handle.api.enable)(handle.pointer.as_ptr());
        }
        Ok(handle)
    }
    #[expect(
        unsafe_code,
        reason = "Connect and publish through the uniquely borrowed initialized instance"
    )]
    pub fn connect(&mut self) -> Result<(), Error> {
        // SAFETY: Handle owns the live instance; the caller holds Client's lock.
        unsafe {
            if (self.api.connect)(self.pointer.as_ptr(), ptr::null_mut()) == 0 {
                return Err(Error::Source("YouTube RTMPS connection failed".into()));
            }
            if (self.api.stream)(self.pointer.as_ptr(), 0) == 0 {
                return Err(Error::Source(
                    "YouTube rejected the publish connection".into(),
                ));
            }
        }
        Ok(())
    }
    #[expect(
        unsafe_code,
        reason = "Read connection state under the same exclusive mutex as writes and destruction"
    )]
    pub fn connected(&self) -> bool {
        // SAFETY: Handle is live; Client serializes this call with all mutations.
        unsafe { (self.api.connected)(self.pointer.as_ptr()) != 0 }
    }
    #[expect(
        unsafe_code,
        reason = "Pass a live NUL-terminated owned FLV batch to synchronous RTMP_Write"
    )]
    pub fn write(&mut self, payload: &[u8]) -> Result<usize, Error> {
        if !self.connected() {
            return Err(Error::Source("YouTube RTMPS connection is closed".into()));
        }
        let length = i32::try_from(payload.len())
            .map_err(|_| Error::Source("RTMP batch too large".into()))?;
        let mut buffer = payload.to_vec();
        buffer.push(0);
        // SAFETY: The writable allocation remains live for the synchronous call;
        // the advertised length excludes its NUL as in ctypes.create_string_buffer.
        // Sink supplies batched complete FLV tags and carries unconsumed tails.
        let written =
            unsafe { (self.api.write)(self.pointer.as_ptr(), buffer.as_ptr().cast(), length) };
        let written = usize::try_from(written)
            .ok()
            .filter(|n| *n > 0 && *n <= payload.len())
            .ok_or_else(|| Error::Source("YouTube RTMPS write failed".into()))?;
        Ok(written)
    }
}
#[expect(
    unsafe_code,
    reason = "Close and free the exclusively owned initialized RTMP allocation exactly once"
)]
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: pointer came from Alloc; no competing call exists. Its library
        // and URL buffers remain alive until both C functions return.
        unsafe {
            (self.api.close)(self.pointer.as_ptr());
            (self.api.free)(self.pointer.as_ptr());
        }
    }
}
fn terminated(value: &str) -> Result<Vec<u8>, Error> {
    if value.contains('\0') {
        return Err(Error::Source("embedded null byte".into()));
    }
    let mut result = value.as_bytes().to_vec();
    result.push(0);
    Ok(result)
}
