use crate::Error;

#[derive(Clone, Copy, Debug)]
pub enum Setup {
    KernelActive,
    Detach,
    Reset,
    Configuration,
    Claim,
    Alternate,
    ClearHalt,
}
impl Setup {
    pub const fn label(self) -> &'static str {
        match self {
            Self::KernelActive => "libusb_kernel_driver_active",
            Self::Detach => "libusb_detach_kernel_driver",
            Self::Reset => "libusb_reset_device",
            Self::Configuration => "libusb_set_configuration",
            Self::Claim => "libusb_claim_interface",
            Self::Alternate => "libusb_set_interface_alt_setting",
            Self::ClearHalt => "libusb_clear_halt",
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Control {
    pub kind: u8,
    pub request: u8,
    pub value: u16,
    pub index: u16,
    pub timeout_ms: u32,
}
#[derive(Debug)]
pub struct Description {
    pub bus: u8,
    pub address: u8,
    pub product: Vec<u8>,
}
#[derive(Clone, Copy, Debug)]
pub struct BulkResult {
    pub code: i32,
    pub actual: u32,
}
#[derive(Debug)]
pub struct Transfer {
    pub endpoint: u8,
    pub stream: Option<u32>,
    pub timeout_ms: u32,
    pub data: Vec<u8>,
    pub status: i32,
    pub actual: u32,
}
impl Transfer {
    pub fn new(endpoint: u8, stream: Option<u32>, data: Vec<u8>) -> Self {
        Self {
            endpoint,
            stream,
            timeout_ms: 1000,
            data,
            status: -1,
            actual: 0,
        }
    }
}
pub trait Transport {
    fn describe(&self) -> Result<Description, Error>;
    fn setup(&mut self, operation: Setup, value: i32, other: i32) -> Result<i32, Error>;
    fn streams(&mut self, endpoints: &[u8], count: u32) -> Result<i32, Error>;
    fn control(&mut self, control: Control, bytes: &mut [u8]) -> Result<i32, Error>;
    fn bulk(
        &mut self,
        endpoint: u8,
        bytes: &mut [u8],
        timeout_ms: u32,
    ) -> Result<BulkResult, Error>;
    fn batch(&mut self, transfers: &mut [Transfer]) -> Result<(), Error>;
    fn error_text(&self, code: i32) -> String;
    fn checked(&self, code: i32, operation: &'static str) -> Result<i32, Error> {
        if code < 0 {
            Err(Error::UsbApi {
                operation,
                code,
                message: self.error_text(code),
            })
        } else {
            Ok(code)
        }
    }
}
