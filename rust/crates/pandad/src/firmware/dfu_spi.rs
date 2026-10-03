use super::{
    spi::{hex, Error, Io, Mode},
    Mcu,
};

pub struct DfuSpi<I> {
    pub io: I,
    pub mcu: Mcu,
}

fn checksum(bytes: &[u8]) -> Result<u8, Error> {
    match bytes {
        [byte] => Ok(byte ^ 0xff),
        [] => Err(Error::Invalid("empty DFU checksum input")),
        _ => Ok(bytes.iter().fold(0, |sum, byte| sum ^ byte)),
    }
}

impl<I: Io> DfuSpi<I> {
    pub fn probe(io: I) -> Result<Self, Error> {
        let mut handle = Self { io, mcu: Mcu::H7 };
        let result = (|| {
            handle.io.lock()?;
            let hello = (|| {
                handle.io.transfer(Mode::Xfer, &[0x5a])?;
                match handle.ack(0.1) {
                    Err(Error::Nack | Error::MissingAck) => Ok(()),
                    result => result,
                }
            })();
            handle.io.unlock()?;
            hello?;
            handle.mcu = match handle.chip_id()? {
                0x463 => Mcu::F4,
                0x483 => Mcu::H7,
                _ => return Err(Error::Invalid("unknown STM32 chip ID")),
            };
            Ok(())
        })();
        match result {
            Err(error) if error.retryable() => {
                Err(Error::Protocol("failed to connect to panda".into()))
            }
            Err(error) => Err(error),
            Ok(()) => Ok(handle),
        }
    }

    pub fn ack(&mut self, timeout: f64) -> Result<(), Error> {
        let mut byte = 0;
        let start = self.io.now()?;
        while !matches!(byte, 0x79 | 0x1f) && self.io.now()? - start < timeout {
            let reply = self.io.transfer(Mode::Xfer, &[0])?;
            byte = *reply
                .first()
                .ok_or(Error::Invalid("empty DFU ACK response"))?;
            self.io.sleep(0.0)?;
        }
        self.io.transfer(Mode::Xfer, &[0x79])?;
        match byte {
            0x79 => Ok(()),
            0x1f => Err(Error::Nack),
            _ => Err(Error::MissingAck),
        }
    }

    pub fn command_once(
        &mut self,
        command: u8,
        data: Option<&[Vec<u8>]>,
        read_bytes: usize,
        predata: Option<&[u8]>,
    ) -> Result<Vec<u8>, Error> {
        self.io.lock()?;
        let result = (|| {
            self.io.transfer(Mode::Xfer, &[0x5a])?;
            self.io.transfer(Mode::Xfer, &[command, command ^ 0xff])?;
            self.ack(0.01)?;
            if let Some(predata) = predata {
                self.io.transfer(Mode::Xfer, predata)?;
                self.ack(1.0)?;
            }
            if let Some(data) = data {
                for part in data {
                    let mut checked = predata.unwrap_or(&[]).to_vec();
                    checked.extend(part);
                    let mut packet = part.clone();
                    packet.push(checksum(&checked)?);
                    self.io.transfer(Mode::Xfer, &packet)?;
                    self.ack(20.0)?;
                }
            }
            let mut reply = Vec::new();
            if read_bytes > 0 {
                let data_reply = self.io.transfer(
                    Mode::Xfer,
                    &vec![
                        0;
                        read_bytes
                            .checked_add(1)
                            .ok_or(Error::Invalid("DFU read size overflow"))?
                    ],
                )?;
                reply.extend(data_reply.get(1..).unwrap_or(&[]));
                if data.is_none_or(|data| data.is_empty()) {
                    self.ack(1.0)?;
                }
            }
            Ok(reply)
        })();
        self.io.unlock()?;
        result
    }

    pub fn command(
        &mut self,
        command: u8,
        data: Option<&[Vec<u8>]>,
        read_bytes: usize,
        predata: Option<&[u8]>,
    ) -> Result<Vec<u8>, Error> {
        let mut last = Error::Protocol(String::new());
        for attempt in 0..5 {
            match self.command_once(command, data, read_bytes, predata) {
                Err(error) if error.retryable() => {
                    self.io.log(
                        format!("SPI transfer failed, {} retries left", 4 - attempt),
                        Some(&error),
                    )?;
                    last = error;
                }
                result => return result,
            }
        }
        Err(last)
    }

    pub fn read(&mut self, address: u32, length: usize) -> Result<Vec<u8>, Error> {
        let count = length
            .checked_sub(1)
            .and_then(|value| u8::try_from(value).ok())
            .ok_or(Error::Invalid("DFU read length must be 1 through 256"))?;
        self.command(
            0x11,
            Some(&[address.to_be_bytes().to_vec(), vec![count]]),
            length,
            None,
        )
    }
    pub fn chip_id(&mut self) -> Result<u16, Error> {
        let reply = self.command(2, None, 3, None)?;
        if reply.first() != Some(&1) {
            return Err(Error::Protocol("incorrect response length".into()));
        }
        let value = reply
            .get(1..3)
            .ok_or(Error::Invalid("short chip ID response"))?;
        Ok(u16::from_be_bytes([value[0], value[1]]))
    }
    pub fn uid(&mut self) -> Result<String, Error> {
        self.read(0x1ff1_e800, 12).map(|bytes| hex(&bytes))
    }
    pub fn erase_sector(&mut self, sector: u16) -> Result<(), Error> {
        self.command(
            0x44,
            Some(&[sector.to_be_bytes().to_vec()]),
            0,
            Some(&[0, 0]),
        )
        .map(|_| ())
    }
    pub fn program(&mut self, address: u32, data: &[u8]) -> Result<(), Error> {
        let mut padded = data.to_vec();
        padded.resize(
            data.len()
                .checked_add(255)
                .ok_or(Error::Invalid("DFU image length overflow"))?
                / 256
                * 256,
            0xff,
        );
        for (index, block) in padded.chunks_exact(256).enumerate() {
            let offset = u32::try_from(index * 256)
                .map_err(|_| Error::Invalid("DFU image address overflow"))?;
            let address = address
                .checked_add(offset)
                .ok_or(Error::Invalid("DFU image address overflow"))?;
            let mut packet = vec![255];
            packet.extend(block);
            self.command(
                0x31,
                Some(&[address.to_be_bytes().to_vec(), packet]),
                0,
                None,
            )?;
        }
        Ok(())
    }
    pub fn jump(&mut self) -> Result<(), Error> {
        self.command(
            0x21,
            Some(&[0x0800_0000_u32.to_be_bytes().to_vec()]),
            0,
            None,
        )
        .map(|_| ())
    }
    pub fn recover(&mut self, code: &[u8]) -> Result<(), Error> {
        for sector in 0..self.mcu.sectors().len() {
            self.erase_sector(sector as u16)?;
        }
        self.program(0x0800_0000, code)?;
        self.jump()
    }
}
