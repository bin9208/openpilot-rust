use crate::{
    clock::Clock,
    transport::Transport,
    usb3::{Command, Usb3},
    Error,
};
use std::collections::HashMap;
#[derive(Clone)]
pub enum Operation {
    Write {
        address: u32,
        data: Vec<u8>,
        ignore_cache: bool,
    },
    Read {
        address: u32,
        size: u8,
    },
    ScsiWrite {
        data: Vec<u8>,
        lba: u64,
    },
}
impl Operation {
    fn write(address: u32, data: impl Into<Vec<u8>>, ignore_cache: bool) -> Self {
        Self::Write {
            address,
            data: data.into(),
            ignore_cache,
        }
    }
}
pub struct StockAsm<T, C> {
    pub usb: Usb3<T, C>,
    cache: HashMap<u32, Option<u8>>,
    pci_cache: HashMap<u64, Option<u32>>,
    cacheable: Vec<(u64, u64)>,
}
impl<T: Transport, C: Clock> StockAsm<T, C> {
    pub fn new(usb: Usb3<T, C>) -> Result<Self, Error> {
        let mut result = Self {
            usb,
            cache: HashMap::new(),
            pci_cache: HashMap::new(),
            cacheable: Vec::new(),
        };
        result.execute(&[
            Operation::write(0x54b, [0x20], true),
            Operation::write(0x54e, [4], true),
            Operation::write(0x5a8, [2], true),
            Operation::write(0x5f8, [4], true),
            Operation::write(0x7ec, [1, 0, 0, 0], true),
            Operation::write(0xc422, [2], true),
            Operation::write(0, [0x33], true),
        ])?;
        Ok(result)
    }
    pub fn execute(&mut self, operations: &[Operation]) -> Result<Vec<Option<Vec<u8>>>, Error> {
        let mut commands = Vec::new();
        for operation in operations {
            match operation {
                Operation::Write {
                    address,
                    data,
                    ignore_cache,
                } => {
                    for (offset, &value) in data.iter().enumerate() {
                        let address = (address.wrapping_add(offset as u32) & 0x1ffff) | 0x500000;
                        if !ignore_cache && self.cache.get(&address) == Some(&Some(value)) {
                            continue;
                        }
                        commands.push(Command {
                            cdb: vec![
                                0xe5,
                                value,
                                (address >> 16) as u8,
                                (address >> 8) as u8,
                                address as u8,
                                0,
                            ],
                            read: 0,
                            write: None,
                        });
                        self.cache.insert(address, Some(value));
                    }
                }
                Operation::Read { address, size } => {
                    let address = (address & 0x1ffff) | 0x500000;
                    commands.push(Command {
                        cdb: vec![
                            0xe4,
                            *size,
                            (address >> 16) as u8,
                            (address >> 8) as u8,
                            address as u8,
                            0,
                        ],
                        read: usize::from(*size),
                        write: None,
                    });
                    for offset in 0..u32::from(*size) {
                        self.cache.insert(address + offset, None);
                    }
                }
                Operation::ScsiWrite { data, lba } => {
                    let length = data
                        .len()
                        .checked_next_multiple_of(512)
                        .ok_or(Error::Contract("SCSI write size overflow"))?;
                    let sectors = u32::try_from(length / 512)
                        .map_err(|_| Error::Contract("SCSI sector count overflow"))?;
                    let mut cdb = vec![0x8a, 0];
                    cdb.extend_from_slice(&lba.to_be_bytes());
                    cdb.extend_from_slice(&sectors.to_be_bytes());
                    cdb.extend_from_slice(&[0, 0]);
                    let mut padded = data.clone();
                    padded.resize(length, 0);
                    commands.push(Command {
                        cdb,
                        read: 0,
                        write: Some(padded),
                    });
                }
            }
        }
        self.usb.send_batch(&commands)
    }
    pub fn write(&mut self, address: u32, data: &[u8], ignore_cache: bool) -> Result<(), Error> {
        self.execute(&[Operation::write(address, data, ignore_cache)])?;
        Ok(())
    }
    pub fn read(&mut self, address: u32, length: usize, stride: u8) -> Result<Vec<u8>, Error> {
        if stride == 0 {
            return Err(Error::Contract("XDATA read stride is zero"));
        }
        let operations = (0..length)
            .step_by(usize::from(stride))
            .map(|offset| Operation::Read {
                address: address.wrapping_add(offset as u32),
                size: (length - offset).min(usize::from(stride)) as u8,
            })
            .collect::<Vec<_>>();
        let mut data = self
            .execute(&operations)?
            .into_iter()
            .flatten()
            .flatten()
            .collect::<Vec<_>>();
        data.truncate(length);
        Ok(data)
    }
    pub fn scsi_write(&mut self, data: &[u8], lba: u64) -> Result<(), Error> {
        let mut data = data.to_vec();
        if data.len() > 0x4000 {
            let length = data
                .len()
                .checked_next_multiple_of(0x10000)
                .ok_or(Error::Contract("SCSI padding overflow"))?;
            data.resize(length, 0);
        }
        for chunk in data.chunks(0x10000) {
            self.execute(&[
                Operation::ScsiWrite {
                    data: chunk.to_vec(),
                    lba,
                },
                Operation::write(0x171, [255, 255, 255], true),
            ])?;
            self.write(0xce6e, &[0, 0], true)?;
        }
        if data.len() > 0x4000 {
            for offset in 0..4 {
                self.write(0xce40 + offset, &[0], true)?;
            }
        }
        Ok(())
    }
    pub fn cache_range(&mut self, address: u64, size: u64) {
        self.cacheable.push((address, size));
    }
    fn prepare(
        &mut self,
        format: u8,
        address: u64,
        value: Option<u32>,
        size: u8,
    ) -> Result<Vec<Operation>, Error> {
        if format == 0x60
            && size == 4
            && self
                .cacheable
                .iter()
                .any(|&(base, size)| address >= base && address - base <= size)
            && self.pci_cache.get(&address) == Some(&value)
        {
            return Ok(Vec::new());
        }
        let offset = (address & 3) as u8;
        if size == 0
            || size > 4
            || size + offset > 4
            || value.is_some_and(|v| u64::from(v) >> (u32::from(size) * 8) != 0)
        {
            return Err(Error::Contract(
                "invalid stock PCIe request alignment or value",
            ));
        }
        self.pci_cache.insert(
            address,
            if size == 4 && format == 0x60 {
                value
            } else {
                None
            },
        );
        let mut operations = Vec::new();
        if let Some(value) = value {
            operations.push(Operation::write(
                0xb220,
                (value << (8 * offset)).to_be_bytes(),
                false,
            ));
        }
        operations.extend([
            Operation::write(0xb218, ((address & 0xfffffffc) as u32).to_be_bytes(), false),
            Operation::write(0xb21c, ((address >> 32) as u32).to_be_bytes(), false),
            Operation::write(0xb217, [(((1u16 << size) - 1) << offset) as u8], false),
            Operation::write(0xb210, [format], false),
            Operation::write(0xb254, [15], true),
            Operation::write(0xb296, [4], true),
        ]);
        Ok(operations)
    }
    pub fn request(
        &mut self,
        format: u8,
        address: u64,
        value: Option<u32>,
        size: u8,
    ) -> Result<Option<u32>, Error> {
        let mut retries = 10;
        'request: loop {
            let operations = self.prepare(format, address, value, size)?;
            self.execute(&operations)?;
            if format & 0b11011111 == 0b01000000 || format & 0b10111000 == 0b00110000 {
                return Ok(None);
            }
            let state = loop {
                let state = self.read(0xb296, 1, 255)?;
                let state = *state
                    .first()
                    .ok_or(Error::Contract("short stock PCIe status"))?;
                if state & 2 != 0 {
                    break state;
                }
                if state & 1 != 0 {
                    self.write(0xb296, &[1], true)?;
                    if retries > 0 {
                        retries -= 1;
                        continue 'request;
                    }
                }
            };
            if state != 2 {
                return Err(Error::Protocol(format!("stat read 2 was {state}")));
            }
            let b284 = self.read(0xb284, 1, 255)?;
            let b284 = *b284
                .first()
                .ok_or(Error::Contract("short stock completion type"))?;
            let completion = self.read(0xb22a, 2, 255)?;
            let completion = u16::from_be_bytes(
                completion
                    .try_into()
                    .map_err(|_| Error::Contract("short stock completion status"))?,
            );
            let config = format & 0xbe == 4;
            if completion & 0xfff != u16::from(if config { 4 } else { size }) {
                return Err(Error::Contract("stock completion byte count mismatch"));
            }
            let status = (completion >> 13) & 7;
            if status != 0 || (config && (value.is_none() != (b284 & 1 != 0))) {
                return Err(Error::Protocol(format!("TLP status: {}",match status{1=>format!("Unsupported Request: invalid address/function (target might not be reachable): {address:#x}"),4=>"Completer Abort: abort due to internal error".into(),2=>"Configuration Request Retry Status: configuration space busy".into(),_=>format!("Reserved (0b{status:03b})")})));
            }
            return if value.is_none() {
                let bytes = self.read(0xb220, 4, 255)?;
                let data = u32::from_be_bytes(
                    bytes
                        .try_into()
                        .map_err(|_| Error::Contract("short stock PCIe read"))?,
                );
                Ok(Some(
                    (data >> (8 * (address & 3))) & (u32::MAX >> (32 - u32::from(size) * 8)),
                ))
            } else {
                Ok(None)
            };
        }
    }
    pub fn memory_write(&mut self, address: u64, values: &[u32], size: u8) -> Result<(), Error> {
        let operations = values
            .iter()
            .enumerate()
            .map(|(index, &value)| {
                self.prepare(
                    0x60,
                    address + index as u64 * u64::from(size),
                    Some(value),
                    size,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        for window in operations.chunks(16) {
            let flattened = window.iter().flatten().cloned().collect::<Vec<_>>();
            self.execute(&flattened)?;
        }
        Ok(())
    }
}
