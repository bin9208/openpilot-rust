use super::{abi::RawTransfer, api::Api, Usb};
use crate::{transport::Transfer, Error};
use std::{cell::Cell, ptr::NonNull, rc::Rc};

struct Pending {
    pointer: NonNull<RawTransfer>,
    api: Rc<Api>,
    finished: Box<Cell<bool>>,
    submitted: bool,
}
impl Pending {
    fn new(api: &Rc<Api>) -> Result<Self, Error> {
        // SAFETY: libusb allocates an aligned transfer with zero isochronous descriptors; its paired free owns teardown.
        let pointer = NonNull::new(unsafe { (api.alloc)(0) }).ok_or(Error::Allocation(64))?;
        Ok(Self {
            pointer,
            api: Rc::clone(api),
            finished: Box::new(Cell::new(false)),
            submitted: false,
        })
    }
    fn active(&self) -> bool {
        self.submitted && !self.finished.get()
    }
}
impl Drop for Pending {
    fn drop(&mut self) {
        // SAFETY: Batch drains all submitted terminal callbacks before dropping Pending; this pointer has one owner.
        unsafe { (self.api.free)(self.pointer.as_ptr()) };
    }
}

// libusb invokes callbacks on the thread handling this context's events. Rc makes
// Usb !Send/!Sync, and only this owner calls handle_events. No callback allocates,
// indexes, returns an error or unwinds; the boxed Cell stays put until drain ends.
unsafe extern "C" fn completed(transfer: *mut RawTransfer) {
    // SAFETY: libusb supplies the submitted transfer still owned by Pending; user_data is its stable boxed Cell.
    let cell = unsafe { (*transfer).user_data.cast::<Cell<bool>>() };
    // SAFETY: the Cell allocation lives through this terminal callback on its only event-handling thread.
    unsafe { &*cell }.set(true);
}

struct Batch<'a> {
    usb: &'a Usb,
    pending: Vec<Pending>,
}
impl Batch<'_> {
    fn drain(&mut self, mut failure: Option<(i32, &'static str)>) -> Option<(i32, &'static str)> {
        let mut cancelled = false;
        while self.pending.iter().any(Pending::active) {
            if failure.is_some() && !cancelled {
                for item in self.pending.iter().filter(|item| item.active()) {
                    // SAFETY: submitted transfers are still live; cancellation is followed by mandatory callback drain.
                    let code = unsafe { (self.usb.api.cancel)(item.pointer.as_ptr()) };
                    if code < 0 && code != -5 {
                        eprintln!("usbgpu: USB cancellation failed; awaiting terminal callback");
                    }
                }
                cancelled = true;
            }
            let mut interval = libc::timeval {
                tv_sec: 0,
                tv_usec: 10000,
            };
            // SAFETY: this !Send owner handles its live context with no other event thread; transfer/data remain owned.
            let code = unsafe {
                (self.usb.api.events)(self.usb.context, &mut interval, std::ptr::null_mut())
            };
            if code < 0 && code != -10 && failure.is_none() {
                failure = Some((code, "libusb_handle_events"));
            }
        }
        failure
    }
}
impl Drop for Batch<'_> {
    fn drop(&mut self) {
        // Also protect cancellation/unwinding paths: a transfer may never be
        // freed while libusb can still access its buffer or completion Cell.
        if self.pending.iter().any(Pending::active) {
            if let Some((code, operation)) = self.drain(Some((-1, "USB batch unwound"))) {
                eprintln!("usbgpu: {operation}: {}", self.usb.api.text(code));
            }
        }
    }
}

impl Usb {
    pub(super) fn batch_owned(&mut self, requests: &mut [Transfer]) -> Result<(), Error> {
        if requests.len() > 124 {
            return Err(Error::Contract("USB batch exceeds 31 four-transfer slots"));
        }
        let mut batch = Batch {
            usb: self,
            pending: Vec::with_capacity(requests.len()),
        };
        for request in requests.iter_mut() {
            let length = i32::try_from(request.data.len())
                .map_err(|_| Error::Contract("USB stream buffer size"))?;
            let mut item = Pending::new(&self.api)?;
            // SAFETY: this aligned transfer has not been submitted; its exclusive fields match checked SDK layout.
            let transfer = unsafe { item.pointer.as_mut() };
            transfer.handle = self.handle;
            transfer.flags = 0;
            transfer.endpoint = request.endpoint;
            transfer.kind = if request.stream.is_some() { 4 } else { 2 };
            transfer.timeout = request.timeout_ms;
            transfer.length = length;
            transfer.callback = Some(completed);
            transfer.user_data = std::ptr::from_ref(item.finished.as_ref()).cast_mut().cast();
            transfer.buffer = request.data.as_mut_ptr();
            transfer.iso_packets = 0;
            if let Some(stream) = request.stream {
                // SAFETY: only the library accesses its hidden stream-id allocation; the transfer is live and not yet submitted.
                unsafe { (self.api.stream_id)(item.pointer.as_ptr(), stream) };
            }
            batch.pending.push(item);
        }
        let mut failure = None;
        for item in &mut batch.pending {
            // SAFETY: transfer, boxed Cell, request buffer, handle, context and API remain alive until drain completes.
            let code = unsafe { (self.api.submit)(item.pointer.as_ptr()) };
            if code < 0 {
                failure = Some((code, "libusb_submit_transfer"));
                break;
            }
            item.submitted = true;
        }
        if let Some((code, operation)) = batch.drain(failure) {
            self.api.checked(code, operation)?;
        }
        for (request, item) in requests.iter_mut().zip(&batch.pending) {
            // SAFETY: terminal callbacks have run; libusb no longer mutates these fields and Pending retains storage.
            let transfer = unsafe { item.pointer.as_ref() };
            let actual = u32::try_from(transfer.actual)
                .map_err(|_| Error::Contract("USB stream length outside buffer"))?;
            if transfer.actual > transfer.length {
                return Err(Error::Contract("USB stream length outside buffer"));
            }
            request.status = transfer.status;
            request.actual = actual;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_callback_marks_its_live_owned_cell() {
        let finished = Box::new(Cell::new(false));
        let mut transfer = RawTransfer {
            handle: std::ptr::null_mut(),
            flags: 0,
            endpoint: 0x82,
            kind: 2,
            timeout: 1000,
            status: 0,
            length: 0,
            actual: 0,
            callback: Some(completed),
            user_data: std::ptr::from_ref(finished.as_ref()).cast_mut().cast(),
            buffer: std::ptr::null_mut(),
            iso_packets: 0,
        };
        assert!(!finished.get());
        // SAFETY: the actual production callback receives this live transfer and its stable boxed Cell on the same thread.
        unsafe { completed(&mut transfer) };
        assert!(finished.get());
    }
}
