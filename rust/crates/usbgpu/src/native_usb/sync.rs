use super::Usb;
use crate::{
    transport::{BulkResult, Control, Description, Setup, Transfer, Transport},
    Error,
};

impl Transport for Usb {
    fn describe(&self) -> Result<Description, Error> {
        self.description()
    }

    fn setup(&mut self, operation: Setup, value: i32, other: i32) -> Result<i32, Error> {
        // SAFETY: all functions have the checked SDK signature and exclusively borrow this live handle.
        let code = match operation {
            Setup::KernelActive => unsafe { (self.api.active)(self.handle, value) },
            Setup::Detach => unsafe { (self.api.detach)(self.handle, value) },
            Setup::Reset => unsafe { (self.api.reset)(self.handle) },
            Setup::Configuration => unsafe { (self.api.configure)(self.handle, value) },
            Setup::Claim => {
                // SAFETY: synchronous claim borrows this owner's live handle.
                let code = unsafe { (self.api.claim)(self.handle, value) };
                if code >= 0 {
                    self.claimed = true;
                }
                code
            }
            Setup::Alternate => unsafe { (self.api.alternate)(self.handle, value, other) },
            Setup::ClearHalt => {
                let endpoint =
                    u8::try_from(value).map_err(|_| Error::Contract("invalid USB endpoint"))?;
                // SAFETY: endpoint width is checked and the synchronous operation borrows a live handle.
                unsafe { (self.api.clear)(self.handle, endpoint) }
            }
        };
        Ok(code)
    }

    fn streams(&mut self, endpoints: &[u8], count: u32) -> Result<i32, Error> {
        if endpoints.len() > 32 {
            return Err(Error::Contract("USB stream endpoint count"));
        }
        let mut endpoints = endpoints.to_vec();
        let length = i32::try_from(endpoints.len())
            .map_err(|_| Error::Contract("USB stream endpoint count"))?;
        // SAFETY: live handle and writable endpoints stay alive for the synchronous alloc_streams call.
        let code =
            unsafe { (self.api.streams)(self.handle, count, endpoints.as_mut_ptr(), length) };
        if code >= 0 {
            self.endpoints = endpoints;
        }
        Ok(code)
    }

    fn control(&mut self, control: Control, bytes: &mut [u8]) -> Result<i32, Error> {
        let length =
            u16::try_from(bytes.len()).map_err(|_| Error::Contract("USB control buffer size"))?;
        // SAFETY: the exclusive buffer has exactly length bytes, remains live, and is not retained after this call.
        let code = unsafe {
            (self.api.control)(
                self.handle,
                control.kind,
                control.request,
                control.value,
                control.index,
                bytes.as_mut_ptr(),
                length,
                control.timeout_ms,
            )
        };
        if code >= 0 && usize::try_from(code).is_ok_and(|count| count > bytes.len()) {
            return Err(Error::Contract("USB control length exceeds buffer"));
        }
        Ok(code)
    }

    fn bulk(&mut self, endpoint: u8, bytes: &mut [u8], timeout: u32) -> Result<BulkResult, Error> {
        let length =
            i32::try_from(bytes.len()).map_err(|_| Error::Contract("USB bulk buffer size"))?;
        let mut actual = 0;
        // SAFETY: the exclusive writable buffer and count remain alive for the synchronous call, without retention.
        let code = unsafe {
            (self.api.bulk)(
                self.handle,
                endpoint,
                bytes.as_mut_ptr(),
                length,
                &mut actual,
                timeout,
            )
        };
        let actual =
            u32::try_from(actual).map_err(|_| Error::Contract("USB bulk length outside buffer"))?;
        if usize::try_from(actual).map_or(true, |size| size > bytes.len()) {
            return Err(Error::Contract("USB bulk length outside buffer"));
        }
        Ok(BulkResult { code, actual })
    }

    fn batch(&mut self, transfers: &mut [Transfer]) -> Result<(), Error> {
        self.batch_owned(transfers)
    }
    fn error_text(&self, code: i32) -> String {
        self.api.text(code)
    }
}
