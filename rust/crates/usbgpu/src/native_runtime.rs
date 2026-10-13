use crate::{
    amd_bus::NativeBus,
    asic::{Asic, BootOptions},
    bus_lock::BusLock,
    clock::WallClock,
    device::PciDevice,
    firmware::FirmwareSource,
    hardware::USB_IDS,
    native_usb::Usb,
    transport::Transport,
    usb3::Usb3,
    Error,
};
use std::path::{Path, PathBuf};

pub type Bus = NativeBus<Usb, WallClock>;

pub struct FirmwareDirectory(pub PathBuf);
impl FirmwareSource for FirmwareDirectory {
    fn load(&mut self, name: &str, _: &str) -> Result<Vec<u8>, Error> {
        if Path::new(name).components().count() != 1 || !name.ends_with(".bin") {
            return Err(Error::Contract("invalid firmware filename"));
        }
        Ok(std::fs::read(self.0.join(name))?)
    }
}

pub fn open(source: &mut impl FirmwareSource) -> Result<Asic<Bus>, Error> {
    let mut selected = None;
    for (vendor, product) in USB_IDS {
        if let Some(usb) = Usb::open(vendor, product, 0)? {
            selected = Some(usb);
            break;
        }
    }
    let usb = selected.ok_or(Error::Contract(
        "AMD:0 does not exist (0 devices available)",
    ))?;
    let description = usb.describe()?;
    let path = PathBuf::from(format!(
        "/tmp/am_usb:{}-{}.lock",
        description.bus, description.address
    ));
    let device_lock = crate::device::acquire_lock(&path)?;
    let lock = BusLock::open(Path::new(crate::bus_lock::DEFAULT_PATH))?;
    let _guard = lock.enter()?;
    let usb = Usb3::new(usb, WallClock::default(), lock.clone(), false)?;
    Asic::boot(
        NativeBus::new(PciDevice::new_locked(usb, device_lock)?)?,
        source,
        BootOptions {
            disable_gmmu: true,
            ..BootOptions::default()
        },
    )
}
