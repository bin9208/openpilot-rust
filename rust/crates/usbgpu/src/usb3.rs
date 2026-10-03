use crate::{
    bus_lock::BusLock,
    clock::Clock,
    transport::{Setup, Transfer, Transport},
    Error,
};
use std::time::Duration;

pub struct Usb3<T, C> {
    pub transport: T,
    pub clock: C,
    pub lock: BusLock,
    pub custom: bool,
    bot: bool,
    tag: u32,
    command_packets: [[u8; 32]; 31],
}
impl<T: Transport, C: Clock> Usb3<T, C> {
    pub fn new(mut transport: T, clock: C, lock: BusLock, use_bot: bool) -> Result<Self, Error> {
        let custom = transport.describe()?.product.starts_with(b"custom");
        let bot = custom || use_bot;
        let active = transport.setup(Setup::KernelActive, 0, 0)?;
        if transport.checked(active, Setup::KernelActive.label())? != 0 {
            for operation in [Setup::Detach, Setup::Reset] {
                let code = transport.setup(operation, 0, 0)?;
                transport.checked(code, operation.label())?;
            }
        }
        for (operation, value, other) in [
            (Setup::Configuration, 1, 0),
            (Setup::Claim, 0, 0),
            (Setup::Alternate, 0, i32::from(!bot)),
        ] {
            let code = transport.setup(operation, value, other)?;
            transport.checked(code, operation.label())?;
        }
        if !bot {
            for ep in [0x02, 0x81, 0x83, 0x04] {
                let code = transport.setup(Setup::ClearHalt, ep, 0)?;
                transport.checked(code, Setup::ClearHalt.label())?;
            }
            let code = transport.streams(&[0x02, 0x81, 0x83], 93)?;
            transport.checked(code, "libusb_alloc_streams")?;
        }
        Ok(Self {
            transport,
            clock,
            lock,
            custom,
            bot,
            tag: 0,
            command_packets: std::array::from_fn(|slot| {
                let mut packet = [0; 32];
                packet[0] = 1;
                packet[3] = slot as u8 + 1;
                packet[16..24].copy_from_slice(&[0xe4, 0x24, 0, 0xb2, 0x1a, 0, 0, 0]);
                packet
            }),
        })
    }
    pub fn bulk_out(&mut self, endpoint: u8, bytes: &[u8], timeout_ms: u32) -> Result<(), Error> {
        let mut payload = bytes.to_vec();
        for attempt in 1..=10 {
            let result = {
                let _guard = self.lock.enter()?;
                self.transport.bulk(endpoint, &mut payload, timeout_ms)?
            };
            if result.code >= 0 {
                if result.actual as usize != bytes.len() {
                    return Err(Error::Protocol(format!(
                        "bulk OUT short write on 0x{endpoint:02X}: {}/{} bytes",
                        result.actual,
                        bytes.len()
                    )));
                }
                return Ok(());
            }
            if result.code != -1 || result.actual != 0 || attempt == 10 {
                return Err(Error::Protocol(format!(
                    "bulk OUT 0x{endpoint:02X} failed after {attempt} attempts ({}/{} bytes): {}",
                    result.actual,
                    bytes.len(),
                    self.transport.error_text(result.code)
                )));
            }
            self.clock.sleep(Duration::from_millis(10));
        }
        unreachable!("bounded retry loop returns on its final attempt")
    }
    pub fn bulk_in(
        &mut self,
        endpoint: u8,
        length: usize,
        timeout_ms: u32,
    ) -> Result<Vec<u8>, Error> {
        let mut data = vec![0; length];
        let result = {
            let _guard = self.lock.enter()?;
            self.transport.bulk(endpoint, &mut data, timeout_ms)?
        };
        self.transport.checked(result.code, "bulk IN failed")?;
        if result.actual as usize > length {
            return Err(Error::Contract("bulk IN actual length exceeds buffer"));
        }
        data.truncate(result.actual as usize);
        Ok(data)
    }
    pub fn send_batch(&mut self, commands: &[Command]) -> Result<Vec<Option<Vec<u8>>>, Error> {
        let lock = self.lock.clone();
        let _guard = lock.enter()?;
        let mut results = Vec::with_capacity(commands.len());
        if self.bot {
            for command in commands {
                if command.cdb.len() > 16 || (command.read > 0 && command.write.is_some()) {
                    return Err(Error::Contract("invalid BOT command"));
                }
                self.tag = self
                    .tag
                    .checked_add(1)
                    .ok_or(Error::Contract("BOT tag overflow"))?;
                let length = if command.read > 0 {
                    command.read
                } else {
                    command.write.as_ref().map_or(0, Vec::len)
                };
                let length = u32::try_from(length)
                    .map_err(|_| Error::Contract("BOT data length overflow"))?;
                let mut cbw = Vec::with_capacity(31);
                cbw.extend_from_slice(&0x43425355u32.to_le_bytes());
                cbw.extend_from_slice(&self.tag.to_le_bytes());
                cbw.extend_from_slice(&length.to_le_bytes());
                cbw.extend_from_slice(&[
                    if command.read > 0 { 0x80 } else { 0 },
                    0,
                    command.cdb.len() as u8,
                ]);
                cbw.extend_from_slice(&command.cdb);
                cbw.resize(31, 0);
                self.bulk_out(0x02, &cbw, 1000)?;
                results.push(if command.read > 0 {
                    Some(self.bulk_in(0x81, command.read, 1000)?)
                } else {
                    if let Some(data) = &command.write {
                        self.bulk_out(0x02, data, 1000)?;
                    }
                    None
                });
                let csw = self.bulk_in(0x81, 13, 2000)?;
                if csw.len() != 13 {
                    return Err(Error::Contract("CSW requires 13 bytes"));
                }
                let signature = u32::from_le_bytes(csw[0..4].try_into().unwrap());
                let tag = u32::from_le_bytes(csw[4..8].try_into().unwrap());
                if signature != 0x53425355 || tag != self.tag || csw[12] != 0 {
                    return Err(Error::Protocol(format!(
                        "invalid CSW signature={signature:08X} tag={tag} status={}",
                        csw[12]
                    )));
                }
            }
        } else {
            for window in commands.chunks(31) {
                let mut transfers = Vec::with_capacity(window.len() * 4);
                let mut read_indices = Vec::with_capacity(window.len());
                for (slot, command) in window.iter().enumerate() {
                    if command.cdb.len() > 16 {
                        return Err(Error::Contract("UAS CDB exceeds 16 bytes"));
                    }
                    let stream = slot as u32 + 1;
                    let packet = &mut self.command_packets[slot];
                    packet[16..16 + command.cdb.len()].copy_from_slice(&command.cdb);
                    transfers.push(Transfer::new(0x04, None, packet.to_vec()));
                    transfers.push(Transfer::new(0x83, Some(stream), vec![0; 64]));
                    read_indices.push(if command.read > 0 {
                        let index = transfers.len();
                        transfers.push(Transfer::new(0x81, Some(stream), vec![0; command.read]));
                        Some(index)
                    } else {
                        None
                    });
                    if let Some(data) = &command.write {
                        transfers.push(Transfer::new(0x02, Some(stream), data.clone()));
                    }
                }
                self.transport.batch(&mut transfers)?;
                for transfer in &transfers {
                    if transfer.status != 0 {
                        return Err(Error::Protocol(format!(
                            "EP 0x{:02X} error: {}",
                            transfer.endpoint, transfer.status
                        )));
                    }
                }
                results.extend(
                    read_indices
                        .into_iter()
                        .map(|index| index.map(|i| std::mem::take(&mut transfers[i].data))),
                );
            }
        }
        Ok(results)
    }
}
pub struct Command {
    pub cdb: Vec<u8>,
    pub read: usize,
    pub write: Option<Vec<u8>>,
}
