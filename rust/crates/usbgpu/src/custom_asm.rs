use crate::{
    clock::Clock,
    transport::{Control, Transport},
    usb3::Usb3,
    Error,
};
use std::{collections::HashMap, time::Duration};
pub struct CustomAsm<T, C> {
    pub usb: Usb3<T, C>,
    cacheable: Vec<(u64, u64)>,
    cache: HashMap<u64, Option<u32>>,
}
impl<T: Transport, C: Clock> CustomAsm<T, C> {
    pub fn new(usb: Usb3<T, C>) -> Result<Self, Error> {
        let mut controller = Self {
            usb,
            cacheable: Vec::new(),
            cache: HashMap::new(),
        };
        let mut state = controller.read(0xb450, 1)?[0];
        if state != 0x78 {
            controller.power(true)?;
            let deadline = controller.usb.clock.now() + Duration::from_secs(2);
            while state != 0x78 && controller.usb.clock.now() < deadline {
                controller.usb.clock.sleep(Duration::from_millis(50));
                state = controller.read(0xb450, 1)?[0];
            }
        }
        if state != 0x78 {
            return Err(Error::Protocol(format!(
                "PCIe link not up (LTSSM=0x{state:02X}), custom firmware not ready"
            )));
        }
        Ok(controller)
    }
    fn control(
        &mut self,
        kind: u8,
        request: u8,
        value: u16,
        index: u16,
        data: &mut [u8],
        timeout_ms: u32,
    ) -> Result<i32, Error> {
        let _guard = self.usb.lock.enter()?;
        self.usb.transport.control(
            Control {
                kind,
                request,
                value,
                index,
                timeout_ms,
            },
            data,
        )
    }
    pub fn power(&mut self, on: bool) -> Result<(), Error> {
        let code = self.control(0x40, 0xf3, u16::from(on), 0, &mut [], 10000)?;
        self.usb.transport.checked(code, "F3 PCIe power failed")?;
        Ok(())
    }
    pub fn read(&mut self, address: u16, length: usize) -> Result<Vec<u8>, Error> {
        let mut result = Vec::with_capacity(length);
        for offset in (0..length).step_by(255) {
            let address = address.wrapping_add(offset as u16);
            let mut data = vec![0; (length - offset).min(255)];
            for attempt in 1..=20 {
                let code = self.control(0xc0, 0xe4, address, 0, &mut data, 1000)?;
                if code == data.len() as i32 {
                    break;
                }
                if code != -1 || attempt == 20 {
                    return Err(Error::Protocol(format!(
                        "read(0x{address:04X}, {}) failed: {code} after {attempt} attempts",
                        data.len()
                    )));
                }
                self.usb.clock.sleep(Duration::from_millis(10));
            }
            result.extend(data);
        }
        Ok(result)
    }
    pub fn write(&mut self, address: u16, data: &[u8]) -> Result<(), Error> {
        for (offset, &value) in data.iter().enumerate() {
            let code = self.control(
                0x40,
                0xe5,
                address.wrapping_add(offset as u16),
                u16::from(value),
                &mut [],
                1000,
            )?;
            self.usb.transport.checked(code, "XDATA write failed")?;
        }
        Ok(())
    }
    fn f0_out(
        &mut self,
        format: u8,
        enable: u8,
        address: u64,
        value: u32,
        mode: u16,
    ) -> Result<(), Error> {
        let mut data = Vec::with_capacity(12);
        data.extend_from_slice(&address.to_le_bytes());
        data.extend_from_slice(&value.to_le_bytes());
        let code = self.control(
            0x40,
            0xf0,
            u16::from(format) | (u16::from(enable) << 8),
            mode & 3,
            &mut data,
            5000,
        )?;
        if code != 12 {
            return Err(Error::Protocol(format!("F0 OUT failed: {code}")));
        }
        Ok(())
    }
    fn f0_in(&mut self) -> Result<(u32, u8, u8), Error> {
        let mut data = [0; 8];
        let code = self.control(0xc0, 0xf0, 0, 0, &mut data, 5000)?;
        if code != 8 {
            return Err(Error::Protocol(format!("F0 IN failed: {code}")));
        }
        Ok((
            u32::from_le_bytes(data[..4].try_into().unwrap()),
            (data[4] >> 5) & 7,
            data[7],
        ))
    }
    pub fn cache_range(&mut self, address: u64, size: u64) {
        self.cacheable.push((address, size));
    }
    pub fn request(
        &mut self,
        format: u8,
        address: u64,
        value: Option<u32>,
        size: u8,
    ) -> Result<Option<u32>, Error> {
        let lock = self.usb.lock.clone();
        let _guard = lock.enter()?;
        if format == 0x60
            && size == 4
            && self
                .cacheable
                .iter()
                .any(|&(base, size)| address >= base && address - base <= size)
            && self.cache.get(&address) == Some(&value)
        {
            return Ok(None);
        }
        if !(1..=4).contains(&size) {
            return Err(Error::Contract("PCIe request size outside 1..4"));
        }
        let offset = (address & 3) as u32;
        let enable = (((1u16 << size) - 1) << offset) as u8;
        let shifted = u64::from(value.unwrap_or(0)) << (8 * offset);
        let shifted = u32::try_from(shifted)
            .map_err(|_| Error::Contract("PCIe request value exceeds dword"))?;
        for attempt in 0..=10 {
            self.cache.insert(
                address,
                if size == 4 && format == 0x60 {
                    value
                } else {
                    None
                },
            );
            self.f0_out(format, enable, address & !3, shifted, 0)?;
            if format & 0b11011111 == 0b01000000 || format & 0b10111000 == 0b00110000 {
                return Ok(None);
            }
            let (data, status, result) = self.f0_in()?;
            if result != 0 {
                self.usb.clock.sleep(Duration::from_millis(1));
                if attempt < 10 {
                    continue;
                }
                return Err(Error::Protocol(format!(
                    "TLP error after retries: ret_status={result}, address={address:#x}"
                )));
            }
            if status != 0 {
                return Err(Error::Protocol(format!(
                    "TLP completion status: {}",
                    match status {
                        1 => format!("Unsupported Request: {address:#x}"),
                        4 => "Completer Abort".into(),
                        2 => "Config Retry".into(),
                        _ => format!("Reserved (0b{status:03b})"),
                    }
                )));
            }
            return Ok(value
                .is_none()
                .then(|| (data >> (8 * offset)) & (u32::MAX >> (32 - u32::from(size) * 8))));
        }
        unreachable!("last TLP attempt returns")
    }
    pub fn memory_write(&mut self, address: u64, values: &[u32]) -> Result<(), Error> {
        if values.is_empty() {
            return Ok(());
        }
        let lock = self.usb.lock.clone();
        let _guard = lock.enter()?;
        self.f0_out(
            0x60,
            15,
            address,
            u32::try_from(values.len())
                .map_err(|_| Error::Contract("PCIe write length overflow"))?,
            1,
        )?;
        let data = values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        self.usb.bulk_out(2, &data, 1000)
    }
    pub fn memory_read(&mut self, address: u64, length: usize) -> Result<Vec<u8>, Error> {
        if !length.is_multiple_of(4) {
            return Err(Error::Contract("PCIe read requires dword-aligned length"));
        }
        let lock = self.usb.lock.clone();
        let _guard = lock.enter()?;
        self.f0_out(
            0x20,
            15,
            address,
            u32::try_from(length / 4).map_err(|_| Error::Contract("PCIe read length overflow"))?,
            2,
        )?;
        self.usb.bulk_in(0x81, length, 30000)
    }
    pub fn scsi_write(&mut self, data: &[u8]) -> Result<(), Error> {
        let length = data
            .len()
            .checked_next_multiple_of(512)
            .ok_or(Error::Contract("SRAM data length overflow"))?;
        let mut padded = data.to_vec();
        padded.resize(length, 0);
        let lock = self.usb.lock.clone();
        let _guard = lock.enter()?;
        let code = self.control(
            0x40,
            0xf2,
            (length / 512) as u16,
            ((length.div_ceil(0x4000) & 255) << 8) as u16,
            &mut [],
            1000,
        )?;
        self.usb.transport.checked(code, "F2 setup failed")?;
        self.usb.bulk_out(2, &padded, 1000)
    }
    pub fn scsi_read_arm(&mut self, length: usize) -> Result<(), Error> {
        let code = self.control(
            0x40,
            0xf2,
            ((length.div_ceil(512) & 0x7fff) | 0x8000) as u16,
            ((length.div_ceil(0x4000) & 255) << 8) as u16,
            &mut [],
            1000,
        )?;
        self.usb.transport.checked(code, "F2 read arm failed")?;
        Ok(())
    }
    pub fn scsi_read(&mut self, length: usize) -> Result<Vec<u8>, Error> {
        let padded = length
            .checked_next_multiple_of(512)
            .ok_or(Error::Contract("SRAM read length overflow"))?;
        let mut data = self.usb.bulk_in(0x81, padded, 10000)?;
        data.truncate(length);
        Ok(data)
    }
}
