use crate::health::{CanHealth, Health};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Control {
    pub request: u8,
    pub value: u16,
    pub index: u16,
    #[serde(default)]
    pub timeout_ms: u32,
}

impl Control {
    pub const fn new(request: u8, value: u16, index: u16) -> Self {
        Self {
            request,
            value,
            index,
            timeout_ms: 0,
        }
    }
}

pub trait Transport {
    type Error;
    fn control_read(&self, command: Control, output: &mut [u8]) -> Result<i32, Self::Error>;
    fn control_write(&self, command: Control) -> Result<i32, Self::Error>;
}

#[derive(Debug, thiserror::Error)]
pub enum Error<E> {
    #[error("Panda transport failed: {0}")]
    Transport(E),
    #[error("Panda returned {returned} bytes for a {capacity}-byte buffer")]
    ReadLength { returned: i32, capacity: usize },
    #[error("Panda firmware out of date. Run pandad.py to update.")]
    FirmwareOutdated,
}

pub struct Device<T> {
    transport: T,
    hardware_type: u8,
    bus_offset: u32,
}

impl<T: Transport> Device<T> {
    pub fn connect(transport: T, bus_offset: u32) -> Result<Self, Error<T::Error>> {
        let mut device = Self {
            transport,
            hardware_type: 0,
            bus_offset,
        };
        let (_, hardware) = device.read::<1>(Control::new(0xc1, 0, 0))?;
        device.hardware_type = hardware[0];
        device.control(Control::new(0xc0, 0, 0))?;
        Ok(device)
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }
    pub const fn hardware_type(&self) -> u8 {
        self.hardware_type
    }
    pub const fn bus_offset(&self) -> u32 {
        self.bus_offset
    }

    pub fn control(&self, command: Control) -> Result<(), Error<T::Error>> {
        self.transport
            .control_write(command)
            .map_err(Error::Transport)?;
        Ok(())
    }

    fn read<const N: usize>(&self, command: Control) -> Result<(i32, [u8; N]), Error<T::Error>> {
        let mut bytes = [0; N];
        let count = self
            .transport
            .control_read(command, &mut bytes)
            .map_err(Error::Transport)?;
        if count > 0 && usize::try_from(count).map_or(true, |count| count > N) {
            return Err(Error::ReadLength {
                returned: count,
                capacity: N,
            });
        }
        Ok((count, bytes))
    }

    pub fn health(&self) -> Result<Option<Health>, Error<T::Error>> {
        let (count, bytes) = self.read(Control::new(0xd2, 0, 0))?;
        Ok((count >= 0).then(|| Health::from_packet(&bytes)))
    }

    pub fn can_health(&self, bus: u16) -> Result<Option<CanHealth>, Error<T::Error>> {
        let (count, bytes) = self.read(Control::new(0xc2, bus, 0))?;
        Ok((count >= 0).then(|| CanHealth::from_packet(&bytes)))
    }

    pub fn fan_speed(&self) -> Result<u16, Error<T::Error>> {
        let (_, bytes) = self.read(Control::new(0xb2, 0, 0))?;
        Ok(u16::from_le_bytes(bytes))
    }

    pub fn serial(&self) -> Result<Option<Vec<u8>>, Error<T::Error>> {
        let (count, bytes) = self.read::<16>(Control::new(0xd0, 0, 0))?;
        Ok((count >= 0).then(|| {
            let end = bytes
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(bytes.len());
            bytes[..end].to_vec()
        }))
    }

    pub fn serial_read(&self, port: u16) -> Result<Vec<u8>, Error<T::Error>> {
        let mut result = Vec::new();
        loop {
            let (count, bytes) = self.read::<64>(Control::new(0xe0, port, 0))?;
            if count <= 0 {
                return Ok(result);
            }
            let length = usize::try_from(count).map_err(|_| Error::ReadLength {
                returned: count,
                capacity: 64,
            })?;
            result.extend_from_slice(&bytes[..length]);
        }
    }

    pub fn firmware_signature(&self) -> Result<Option<[u8; 128]>, Error<T::Error>> {
        let (first_count, first) = self.read::<64>(Control::new(0xd3, 0, 0))?;
        let (second_count, second) = self.read::<64>(Control::new(0xd4, 0, 0))?;
        if first_count != 64 || second_count != 64 {
            return Ok(None);
        }
        let mut signature = [0; 128];
        signature[..64].copy_from_slice(&first);
        signature[64..].copy_from_slice(&second);
        Ok(Some(signature))
    }

    pub fn configure(
        &self,
        loopback: bool,
        skip_firmware_check: bool,
        mut read_file: impl FnMut(&str) -> Vec<u8>,
    ) -> Result<(), Error<T::Error>> {
        if loopback {
            self.control(Control::new(0xe5, 1, 0))?;
        }
        for bus in 0..3 {
            self.control(Control::new(0xe8, bus, 1))?;
        }
        let mut matches = false;
        if let Some(signature) = self.firmware_signature()? {
            'paths: for directory in ["../../../panda/board/obj/", "../../panda/board/obj/"] {
                for filename in ["panda.bin.signed", "panda_h7.bin.signed"] {
                    let content = read_file(&format!("{directory}{filename}"));
                    if content.ends_with(&signature) {
                        matches = true;
                        break 'paths;
                    }
                }
            }
        }
        if !matches && !skip_firmware_check {
            return Err(Error::FirmwareOutdated);
        }
        Ok(())
    }
}
