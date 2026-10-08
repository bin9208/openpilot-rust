use super::{
    client::{Connection, Handle},
    Mcu, Request, Transport,
};
use crate::supervisor::Fault;
use openpilot_panda_usb::{raw, Api, Error};
use std::sync::Arc;

pub enum Log {
    Opening { serial: String, product: u16 },
    InvalidSerial(String),
    Exception(&'static str, Fault),
}

pub fn fault(error: Error) -> Fault {
    match error {
        Error::Usb { code: -4, .. } => Fault::NoDevice(error.to_string()),
        Error::Usb { code: -9, .. } => Fault::Pipe(error.to_string()),
        error => Fault::Other(error.to_string()),
    }
}

pub struct UsbHandle {
    raw: Option<raw::Handle>,
}
impl UsbHandle {
    fn new(raw: raw::Handle) -> Self {
        Self { raw: Some(raw) }
    }
    fn raw(&mut self) -> Result<&mut raw::Handle, Fault> {
        self.raw
            .as_mut()
            .ok_or_else(|| Fault::Other("USB handle is closed".into()))
    }
}
impl Transport for UsbHandle {
    type Error = Fault;
    fn control_read(&mut self, request: Request, length: usize) -> Result<Vec<u8>, Fault> {
        self.raw()?
            .control_read(
                request.kind,
                request.request,
                request.value,
                request.index,
                length,
                request.timeout_ms,
            )
            .map_err(fault)
    }
    fn control_write(&mut self, request: Request, data: &[u8]) -> Result<(), Fault> {
        self.raw()?
            .control_write(
                request.kind,
                request.request,
                request.value,
                request.index,
                data,
                request.timeout_ms,
            )
            .map(|_| ())
            .map_err(fault)
    }
    fn bulk_write(&mut self, endpoint: u8, data: &[u8], timeout_ms: u32) -> Result<(), Fault> {
        self.raw()?
            .bulk_write(endpoint, data, timeout_ms)
            .map(|_| ())
            .map_err(fault)
    }
}
impl Handle for UsbHandle {
    fn close(&mut self) -> Result<(), Fault> {
        self.raw.take();
        Ok(())
    }
}

fn panda(descriptor: raw::DeviceDescriptor) -> bool {
    matches!(descriptor.vendor, 0xbbaa | 0x3801) && matches!(descriptor.product, 0xddee | 0xddcc)
}

pub fn connect(
    api: Arc<Api>,
    serial: &str,
    claim: bool,
    no_error: bool,
    mut log: impl FnMut(Log) -> Result<(), Fault>,
) -> Result<Option<Connection<UsbHandle>>, Fault> {
    let context = raw::Context::new(api).map_err(fault)?;
    let mut selected = None;
    let result = (|| {
        let devices = context.devices().map_err(fault)?;
        for index in 0..devices.len() {
            let device = devices
                .get(index)
                .ok_or_else(|| Fault::Other("USB device entry is null".into()))?;
            let Ok(descriptor) = device.descriptor() else {
                continue;
            };
            if !panda(descriptor) {
                continue;
            }
            let reported = (|| device.open()?.ascii_string(descriptor.serial_index))();
            let reported = match reported {
                Ok(value) => value,
                Err(error) => {
                    if !no_error {
                        log(Log::Exception(
                            "failed to get serial number of panda",
                            fault(error),
                        ))?;
                    }
                    continue;
                }
            };
            if reported.as_deref() != Some(serial) {
                continue;
            }
            log(Log::Opening {
                serial: serial.into(),
                product: descriptor.product,
            })?;
            let handle = device.open().map_err(fault)?;
            selected = Some(Connection {
                handle: UsbHandle::new(handle),
                serial: serial.into(),
                bootstub: descriptor.product & 0xf0 == 0xe0,
                bcd: None,
                spi: false,
            });
            let connection = selected.as_mut().expect("selected handle was assigned");
            connection.handle.raw()?.auto_detach(true).map_err(fault)?;
            if claim {
                connection.handle.raw()?.claim(0).map_err(fault)?;
            }
            if descriptor.bcd != 0x2300 {
                connection.bcd = Some(vec![(descriptor.bcd >> 8) as u8]);
            }
            break;
        }
        Ok(())
    })();
    if let Err(error) = result {
        log(Log::Exception("USB connect error", error))?;
    }
    Ok(selected)
}

pub fn list(
    api: Arc<Api>,
    mut log: impl FnMut(Log) -> Result<(), Fault>,
) -> Result<Vec<String>, Fault> {
    let mut serials = Vec::new();
    let result = (|| {
        let context = raw::Context::new(api).map_err(fault)?;
        let devices = context.devices().map_err(fault)?;
        for index in 0..devices.len() {
            let device = devices
                .get(index)
                .ok_or_else(|| Fault::Other("USB device entry is null".into()))?;
            let Ok(descriptor) = device.descriptor() else {
                continue;
            };
            if !panda(descriptor) {
                continue;
            }
            let reported = (|| device.open()?.ascii_string(descriptor.serial_index))();
            match reported {
                Ok(Some(serial)) if serial.len() == 24 => serials.push(serial),
                Ok(Some(serial)) => log(Log::InvalidSerial(serial))?,
                Ok(None) => log(Log::Exception(
                    "error connecting to panda",
                    Fault::Other("Panda serial descriptor is absent".into()),
                ))?,
                Err(error) => log(Log::Exception("error connecting to panda", fault(error)))?,
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        log(Log::Exception("exception while listing pandas", error))?;
    }
    Ok(serials)
}

pub fn descriptor_mcu(descriptor: &str) -> Result<Mcu, Fault> {
    if !descriptor.starts_with("@Internal Flash") {
        return Err(Fault::Other(
            "DFU internal flash descriptor is missing".into(),
        ));
    }
    let mut count = 0_i64;
    for sector in descriptor.rsplit('/').next().unwrap_or("").split(',') {
        let value = sector
            .split('*')
            .next()
            .unwrap_or("")
            .trim()
            .parse::<i64>()
            .map_err(|error| Fault::Other(error.to_string()))?;
        count = count
            .checked_add(value)
            .ok_or_else(|| Fault::Other("DFU sector count overflow".into()))?;
    }
    match count {
        16 => Ok(Mcu::F4),
        8 => Ok(Mcu::H7),
        _ => Err(Fault::Other(format!("Unknown MCU: sector_count={count}"))),
    }
}

pub fn dfu_connect(api: Arc<Api>, serial: Option<&str>) -> Result<Option<(UsbHandle, Mcu)>, Fault> {
    let context = raw::Context::new(api).map_err(fault)?;
    let devices = context.devices().map_err(fault)?;
    for index in 0..devices.len() {
        let device = devices
            .get(index)
            .ok_or_else(|| Fault::Other("USB device entry is null".into()))?;
        let Ok(descriptor) = device.descriptor() else {
            continue;
        };
        if descriptor.vendor != 0x0483 || descriptor.product != 0xdf11 {
            continue;
        }
        let Ok(reported) = (|| device.open()?.ascii_string(3))() else {
            continue;
        };
        if serial.is_some() && reported.as_deref() != serial {
            continue;
        }
        let mut handle = device.open().map_err(fault)?;
        for descriptor_index in 0..20 {
            let text = handle.string(descriptor_index, 0).map_err(fault)?;
            if let Some(text) = text.filter(|text| text.starts_with("@Internal Flash")) {
                let mcu = descriptor_mcu(&text)?;
                return Ok(Some((UsbHandle::new(handle), mcu)));
            }
        }
        return Err(Fault::Other(
            "DFU internal flash descriptor is missing".into(),
        ));
    }
    Ok(None)
}

pub fn dfu_list(api: Arc<Api>) -> Vec<Option<String>> {
    let mut serials = Vec::new();
    let _ = (|| {
        let context = raw::Context::new(api)?;
        let devices = context.devices()?;
        for index in 0..devices.len() {
            let Some(device) = devices.get(index) else {
                return Err(Error::Contract("USB device entry is null"));
            };
            let Ok(descriptor) = device.descriptor() else {
                continue;
            };
            if descriptor.vendor != 0x0483 || descriptor.product != 0xdf11 {
                continue;
            }
            if let Ok(serial) = (|| device.open()?.ascii_string(3))() {
                serials.push(serial);
            }
        }
        Ok(())
    })();
    serials
}
