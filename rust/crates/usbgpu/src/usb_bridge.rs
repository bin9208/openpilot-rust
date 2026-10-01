#[cxx::bridge(namespace = "openpilot_usbgpu")]
pub(crate) mod ffi {
    pub struct UsbDescription {
        pub bus: u8,
        pub address: u8,
        pub product: Vec<u8>,
    }
    pub struct UsbBulkResult {
        pub code: i32,
        pub actual: u32,
    }
    pub struct UsbTransfer {
        pub endpoint: u8,
        pub stream: u32,
        pub use_stream: bool,
        pub timeout_ms: u32,
        pub data: Vec<u8>,
        pub status: i32,
        pub actual: u32,
    }
    #[repr(u8)]
    pub enum UsbSetup {
        KernelActive,
        Detach,
        Reset,
        Configuration,
        Claim,
        Alternate,
        ClearHalt,
    }
    // SAFETY: CXX validates slice ownership; calls are synchronous and retain no Rust pointers.
    // Native async transfers finish or cancel before returning, and the context outlives its handle.
    unsafe extern "C++" {
        include!("usb.h");
        pub type NativeUsb;
        pub fn open_usb(vendor: u16, product: u16, index: u32) -> Result<UniquePtr<NativeUsb>>;
        pub fn describe(self: &NativeUsb) -> Result<UsbDescription>;
        pub fn setup(
            self: Pin<&mut NativeUsb>,
            operation: UsbSetup,
            value: i32,
            other: i32,
        ) -> Result<i32>;
        pub fn streams(self: Pin<&mut NativeUsb>, endpoints: &[u8], count: u32) -> Result<i32>;
        pub fn control(
            self: Pin<&mut NativeUsb>,
            kind: u8,
            request: u8,
            value: u16,
            index: u16,
            bytes: &mut [u8],
            timeout: u32,
        ) -> Result<i32>;
        pub fn bulk(
            self: Pin<&mut NativeUsb>,
            endpoint: u8,
            bytes: &mut [u8],
            timeout: u32,
        ) -> Result<UsbBulkResult>;
        pub fn batch(self: Pin<&mut NativeUsb>, transfers: &mut [UsbTransfer]) -> Result<()>;
        pub fn error_text(self: &NativeUsb, code: i32) -> String;
    }
}
