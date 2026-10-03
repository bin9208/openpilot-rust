use crate::{
    transport::{BulkResult, Control, Description, Setup, Transfer, Transport},
    usb_bridge::ffi,
    Error,
};

pub struct Usb(cxx::UniquePtr<ffi::NativeUsb>);
impl Usb {
    pub fn open(vendor: u16, product: u16, index: u32) -> Result<Option<Self>, Error> {
        let handle = ffi::open_usb(vendor, product, index)?;
        Ok((!handle.is_null()).then_some(Self(handle)))
    }
}
impl Transport for Usb {
    fn describe(&self) -> Result<Description, Error> {
        let source = self.0.describe()?;
        Ok(Description {
            bus: source.bus,
            address: source.address,
            product: source.product,
        })
    }
    fn setup(&mut self, operation: Setup, value: i32, other: i32) -> Result<i32, Error> {
        let operation = match operation {
            Setup::KernelActive => ffi::UsbSetup::KernelActive,
            Setup::Detach => ffi::UsbSetup::Detach,
            Setup::Reset => ffi::UsbSetup::Reset,
            Setup::Configuration => ffi::UsbSetup::Configuration,
            Setup::Claim => ffi::UsbSetup::Claim,
            Setup::Alternate => ffi::UsbSetup::Alternate,
            Setup::ClearHalt => ffi::UsbSetup::ClearHalt,
        };
        Ok(self.0.pin_mut().setup(operation, value, other)?)
    }
    fn streams(&mut self, endpoints: &[u8], count: u32) -> Result<i32, Error> {
        Ok(self.0.pin_mut().streams(endpoints, count)?)
    }
    fn control(&mut self, control: Control, bytes: &mut [u8]) -> Result<i32, Error> {
        Ok(self.0.pin_mut().control(
            control.kind,
            control.request,
            control.value,
            control.index,
            bytes,
            control.timeout_ms,
        )?)
    }
    fn bulk(
        &mut self,
        endpoint: u8,
        bytes: &mut [u8],
        timeout_ms: u32,
    ) -> Result<BulkResult, Error> {
        let result = self.0.pin_mut().bulk(endpoint, bytes, timeout_ms)?;
        Ok(BulkResult {
            code: result.code,
            actual: result.actual,
        })
    }
    fn batch(&mut self, transfers: &mut [Transfer]) -> Result<(), Error> {
        let mut native = transfers
            .iter_mut()
            .map(|transfer| ffi::UsbTransfer {
                endpoint: transfer.endpoint,
                stream: transfer.stream.unwrap_or(0),
                use_stream: transfer.stream.is_some(),
                timeout_ms: transfer.timeout_ms,
                data: std::mem::take(&mut transfer.data),
                status: -1,
                actual: 0,
            })
            .collect::<Vec<_>>();
        let result = self.0.pin_mut().batch(&mut native);
        for (target, source) in transfers.iter_mut().zip(native) {
            target.data = source.data;
            target.status = source.status;
            target.actual = source.actual;
        }
        Ok(result?)
    }
    fn error_text(&self, code: i32) -> String {
        self.0.error_text(code)
    }
}
