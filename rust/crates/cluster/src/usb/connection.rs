use crate::Error;
use openpilot_usbgpu::{
    bus_lock::BusLock,
    native_usb::{Configuration, Usb},
    transport::{Setup, Transport},
};

pub(super) struct Connection {
    pub usb: Option<Usb>,
    pub product: u16,
    configuration: Configuration,
    endpoints: Endpoints,
    bus: BusLock,
}

enum Endpoints {
    Uncached,
    Output(u8),
    Ready { output: u8, input: u8 },
}

impl Connection {
    pub fn open(bus: &BusLock, expected: Option<u16>) -> Result<Self, Error> {
        let products: &[u16] = match expected.as_ref() {
            Some(product) => std::slice::from_ref(product),
            None => &[0x92, 0x123],
        };
        for &product in products {
            let guard = bus.enter()?;
            let Some(mut usb) = Usb::open(0x1cbe, product, 0)? else {
                continue;
            };
            if usb.setup(Setup::KernelActive, 0, 0)? == 1 {
                // A driver-detach error is optional in the source; configuration reports access errors.
                if let Err(error) = usb.setup(Setup::Detach, 0, 0) {
                    eprintln!("Warning: detach_kernel_driver failed: {error}");
                }
            }
            let configuration = usb.configuration(0)?;
            let code = usb.setup(Setup::Configuration, i32::from(configuration.value), 0)?;
            usb.checked(code, Setup::Configuration.label())?;
            let code = usb.setup(Setup::Claim, 0, 0)?;
            usb.checked(code, Setup::Claim.label())?;
            drop(guard);
            return Ok(Self {
                usb: Some(usb),
                product,
                configuration,
                endpoints: Endpoints::Uncached,
                bus: bus.clone(),
            });
        }
        Err(Error::Contract("USB device not found"))
    }

    pub fn dimensions(&self) -> Result<[u32; 2], Error> {
        match self.product {
            0x28 => Ok([480, 480]),
            0x46 => Ok([960, 320]),
            0x50 => Ok([1280, 720]),
            0x80 => Ok([1280, 800]),
            0x88 => Ok([1920, 480]),
            0x92 => Ok([1920, 462]),
            0x123 => Ok([1920, 720]),
            _ => Err(Error::Contract(
                "TURZX vendor library does not know product",
            )),
        }
    }

    pub fn cache(&mut self) -> Result<(), Error> {
        let interface = self
            .configuration
            .interfaces
            .iter()
            .find(|interface| interface.number == 0)
            .ok_or(Error::Contract("USB interface 0 not found"))?;
        let output = interface
            .endpoints
            .iter()
            .find(|endpoint| endpoint.address & 0x80 == 0)
            .ok_or(Error::Contract("Could not find USB OUT endpoint"))?
            .address;
        self.endpoints = Endpoints::Output(output);
        let input = interface
            .endpoints
            .iter()
            .find(|endpoint| endpoint.address & 0x80 != 0)
            .ok_or(Error::Contract("Could not find USB IN endpoint"))?
            .address;
        self.endpoints = Endpoints::Ready { output, input };
        Ok(())
    }

    pub const fn output(&self) -> Option<u8> {
        match self.endpoints {
            Endpoints::Uncached => None,
            Endpoints::Output(output) | Endpoints::Ready { output, .. } => Some(output),
        }
    }
    pub const fn input(&self) -> Option<u8> {
        match self.endpoints {
            Endpoints::Uncached | Endpoints::Output(_) => None,
            Endpoints::Ready { input, .. } => Some(input),
        }
    }

    pub fn setup(&mut self, operation: Setup, value: i32) -> Result<(), Error> {
        let guard = self.bus.enter()?;
        let usb = self
            .usb
            .as_mut()
            .ok_or(Error::Contract("USB display is not open"))?;
        let code = usb.setup(operation, value, 0)?;
        usb.checked(code, operation.label())?;
        drop(guard);
        Ok(())
    }

    pub fn bulk(&mut self, endpoint: u8, bytes: &mut [u8], timeout: u32) -> Result<usize, Error> {
        let guard = self.bus.enter()?;
        let usb = self
            .usb
            .as_mut()
            .ok_or(Error::Contract("USB display is not open"))?;
        let result = usb.bulk(endpoint, bytes, timeout)?;
        usb.checked(result.code, "libusb_bulk_transfer")?;
        drop(guard);
        usize::try_from(result.actual).map_err(|_| Error::Contract("USB bulk length overflow"))
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        match self.bus.enter() {
            Ok(_guard) => drop(self.usb.take()),
            Err(error) => eprintln!("Warning: USB display release bus lock failed: {error}"),
        }
    }
}
