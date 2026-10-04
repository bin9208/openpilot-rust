use crate::bridge::ffi;
use openpilot_panda_spi::linux_io::OptionKind;
use std::io;

pub struct FirmwareKernel {
    handle: cxx::UniquePtr<ffi::Handle>,
}
fn checked(call: ffi::Call) -> io::Result<usize> {
    if call.result < 0 {
        Err(io::Error::from_raw_os_error(call.error_number))
    } else {
        Ok(call.result as usize)
    }
}
impl FirmwareKernel {
    pub fn sleep_zero() {
        ffi::sleep_us(0);
    }
    pub fn at_path(path: &str) -> Self {
        cxx::let_cxx_string!(path = path);
        Self {
            handle: ffi::create(&path),
        }
    }
    pub fn exists(&self) -> bool {
        self.handle.exists()
    }
    pub fn open(&mut self) -> io::Result<()> {
        checked(self.handle.pin_mut().open_call()).map(|_| ())
    }
    pub fn read_option(&mut self, option: OptionKind) -> io::Result<u32> {
        let call = self.handle.pin_mut().read_option(match option {
            OptionKind::Mode => 0,
            OptionKind::Speed => 1,
            OptionKind::Bits => 2,
        });
        checked(call.call)?;
        Ok(call.value)
    }
    pub fn set_speed(&mut self, speed: u32) -> io::Result<()> {
        checked(self.handle.pin_mut().configure(1, speed)).map(|_| ())
    }
    pub fn transfer(&mut self, tx: &[u8], rx: &mut [u8], speed: u32, bits: u8) -> io::Result<()> {
        checked(self.handle.pin_mut().transfer_at_speed(tx, rx, speed, bits)).map(|_| ())
    }
    pub fn read(&mut self, rx: &mut [u8]) -> io::Result<usize> {
        checked(self.handle.pin_mut().read_bytes(rx))
    }
    pub fn write(&mut self, tx: &[u8]) -> io::Result<usize> {
        checked(self.handle.pin_mut().write_bytes(tx))
    }
    pub fn kernel_transfer(
        &mut self,
        endpoint: u8,
        tx: &[u8],
        rx: &mut [u8],
        disconnect: bool,
    ) -> io::Result<usize> {
        checked(
            self.handle
                .pin_mut()
                .firmware_transfer(endpoint, tx, rx, disconnect),
        )
    }
    pub fn flock(&mut self, exclusive: bool) -> io::Result<()> {
        checked(self.handle.pin_mut().flock_call(exclusive)).map(|_| ())
    }
    pub fn close(&mut self) {
        self.handle.pin_mut().close();
    }
}
