use super::{client::Handle, Request, Transport};
use crate::supervisor::Fault;

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("SPI NACK")]
    Nack,
    #[error("SPI ACK missing")]
    MissingAck,
    #[error("SPI checksum mismatch")]
    BadChecksum,
    #[error("{0}")]
    Protocol(String),
    #[error("SPI I/O: {0}")]
    Io(String),
    #[error("SPI invalid input: {0}")]
    Invalid(&'static str),
    #[error("panda protocol mismatch: expected 2, got {0}. reflash panda")]
    Version(u8),
}
impl Error {
    pub fn retryable(&self) -> bool {
        matches!(
            self,
            Self::Nack
                | Self::MissingAck
                | Self::BadChecksum
                | Self::Protocol(_)
                | Self::Version(_)
        )
    }
    pub fn into_fault(self) -> Fault {
        match self {
            Self::Version(_) => Fault::Protocol(self.to_string()),
            _ => Fault::Other(self.to_string()),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Mode {
    Xfer,
    Xfer2,
}

pub trait Io {
    fn lock(&mut self) -> Result<(), Error>;
    fn unlock(&mut self) -> Result<(), Error>;
    fn transfer(&mut self, mode: Mode, data: &[u8]) -> Result<Vec<u8>, Error>;
    fn read(&mut self, length: usize) -> Result<Vec<u8>, Error>;
    fn write(&mut self, data: &[u8]) -> Result<(), Error>;
    fn now(&mut self) -> Result<f64, Error>;
    fn sleep(&mut self, seconds: f64) -> Result<(), Error>;
    fn log(&mut self, text: String, exception: Option<&Error>) -> Result<(), Error>;
    fn kernel(
        &mut self,
        endpoint: u8,
        data: &[u8],
        maximum: usize,
        disconnect: bool,
    ) -> Result<Vec<u8>, Error>;
}

fn checksum(data: &[u8]) -> u8 {
    data.iter().fold(0xab, |result, byte| result ^ byte)
}
pub fn crc8(data: &[u8]) -> u8 {
    let mut crc = 0xff_u8;
    for byte in data.iter().rev() {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0xd5
            } else {
                crc << 1
            };
        }
    }
    crc
}

#[derive(Debug)]
pub struct Identity {
    pub serial: String,
    pub bootstub: bool,
    pub version: u8,
}
pub struct PandaSpi<I> {
    pub io: I,
    kernel: bool,
}
impl<I: Io> PandaSpi<I> {
    pub fn new(io: I, kernel: bool) -> Self {
        Self { io, kernel }
    }

    pub fn wait_ack(
        &mut self,
        ack: u8,
        timeout: u32,
        tx: u8,
        length: usize,
    ) -> Result<Vec<u8>, Error> {
        let timeout_s = f64::from(timeout.max(100)) * 1e-3;
        let start = self.io.now()?;
        while timeout == 0 || self.io.now()? - start < timeout_s {
            let reply = self.io.transfer(Mode::Xfer2, &vec![tx; length])?;
            match reply.first() {
                Some(0x1f) => return Err(Error::Nack),
                Some(value) if *value == ack => return Ok(reply),
                Some(_) => (),
                None => return Err(Error::Invalid("empty SPI ACK response")),
            }
        }
        Err(Error::MissingAck)
    }

    pub fn transfer_spidev(
        &mut self,
        endpoint: u8,
        data: &[u8],
        timeout: u32,
        maximum: usize,
        disconnect: bool,
    ) -> Result<Vec<u8>, Error> {
        let maximum = maximum.max(64);
        self.io.log("- send header".into(), None)?;
        let length = u16::try_from(data.len())
            .map_err(|_| Error::Invalid("SPI data length exceeds uint16"))?;
        let maximum_u16 = u16::try_from(maximum)
            .map_err(|_| Error::Invalid("SPI response length exceeds uint16"))?;
        let mut header = vec![0x5a, endpoint];
        header.extend(length.to_le_bytes());
        header.extend(maximum_u16.to_le_bytes());
        header.push(checksum(&header));
        self.io.transfer(Mode::Xfer2, &header)?;
        self.io.log("- waiting for header ACK".into(), None)?;
        self.wait_ack(0x79, 100, 0x11, 1)?;
        self.io.log("- sending data".into(), None)?;
        let mut packet = data.to_vec();
        packet.push(checksum(data));
        self.io.transfer(Mode::Xfer2, &packet)?;
        if disconnect {
            self.io
                .log("- expecting disconnect, returning".into(), None)?;
            return Ok(Vec::new());
        }
        self.io.log("- waiting for data ACK".into(), None)?;
        let mut reply = self.wait_ack(0x85, timeout, 0x13, 68)?;
        let length = reply
            .get(1..3)
            .ok_or(Error::Invalid("short SPI response length"))?;
        let length = usize::from(u16::from_le_bytes([length[0], length[1]]));
        if length > maximum {
            return Err(Error::Protocol(format!(
                "response length greater than max ({maximum} {length})"
            )));
        }
        if length + 1 > 65 {
            reply.extend(self.io.read(length + 1 - 65)?);
        }
        reply.truncate(3 + length + 1);
        if checksum(&reply) != 0 {
            return Err(Error::BadChecksum);
        }
        Ok(reply
            .get(3..reply.len().saturating_sub(1))
            .ok_or(Error::Invalid("short SPI response"))?
            .to_vec())
    }

    pub fn transfer(
        &mut self,
        endpoint: u8,
        data: &[u8],
        timeout: u32,
        maximum: usize,
        disconnect: bool,
    ) -> Result<Vec<u8>, Error> {
        self.io.log(
            format!("starting transfer: endpoint={endpoint}, max_rx_len={maximum}"),
            None,
        )?;
        self.io.log(
            "==============================================".into(),
            None,
        )?;
        let mut attempts = 0_u64;
        let start = self.io.now()?;
        let mut last_error = Error::Protocol(String::new());
        while timeout == 0 || self.io.now()? - start < f64::from(timeout) * 1e-3 {
            attempts += 1;
            self.io.log(format!("\ntry #{attempts}"), None)?;
            self.io.lock()?;
            let result = if self.kernel {
                self.io.kernel(endpoint, data, maximum, disconnect)
            } else {
                self.transfer_spidev(endpoint, data, timeout, maximum, disconnect)
            };
            let log_result = match &result {
                Err(error) if error.retryable() => self
                    .io
                    .log("SPI transfer failed, retrying".into(), Some(error)),
                _ => Ok(()),
            };
            self.io.unlock()?;
            log_result?;
            match result {
                Err(error) if error.retryable() => last_error = error,
                result => return result,
            }
        }
        Err(last_error)
    }

    fn protocol_once(&mut self) -> Result<Vec<u8>, Error> {
        self.io.write(b"VERSION")?;
        self.io.log("- waiting for echo".into(), None)?;
        let start = self.io.now()?;
        let version = loop {
            let value = self.io.read(9)?;
            if value.starts_with(b"VERSION") {
                break value;
            }
            if self.io.now()? - start > 0.001 {
                return Err(Error::MissingAck);
            }
        };
        let length_bytes = version
            .get(version.len().saturating_sub(2)..)
            .ok_or(Error::Invalid("short version length"))?;
        let length = usize::from(u16::from_le_bytes([length_bytes[0], length_bytes[1]]));
        if length > 1000 {
            return Err(Error::Protocol("response length greater than max".into()));
        }
        let data = self.io.read(length + 1)?;
        let (&actual_crc, response) = data
            .split_last()
            .ok_or(Error::Invalid("missing version checksum"))?;
        let mut check = version;
        check.extend(response);
        if crc8(&check) != actual_crc {
            return Err(Error::BadChecksum);
        }
        Ok(response.to_vec())
    }

    pub fn protocol_version(&mut self) -> Result<Vec<u8>, Error> {
        self.io.lock()?;
        let result = (|| {
            let mut last = Error::Protocol(String::new());
            for _ in 0..10 {
                match self.protocol_once() {
                    Err(error) if error.retryable() => {
                        self.io.log(
                            "SPI get protocol version failed, retrying".into(),
                            Some(&error),
                        )?;
                        last = error;
                    }
                    result => return result,
                }
            }
            Err(last)
        })();
        self.io.unlock()?;
        result
    }

    pub fn identify(
        &mut self,
        serial: Option<&str>,
        ignore_version: bool,
    ) -> Result<Option<Identity>, Error> {
        let result = (|| {
            let current = self.protocol_version().and_then(|reply| {
                let pid = *reply
                    .get(13)
                    .ok_or(Error::Invalid("short SPI protocol identity"))?;
                if !matches!(pid, 0xcc | 0xee) {
                    return Err(Error::Protocol("invalid bootstub status".into()));
                }
                let version = *reply
                    .get(14)
                    .ok_or(Error::Invalid("missing SPI protocol version"))?;
                Ok(Identity {
                    serial: hex(&reply[..12]),
                    bootstub: pid == 0xee,
                    version,
                })
            });
            match current {
                Err(error) if error.retryable() => {
                    let mut request = Request::new(0xc0, 0xc3, 0);
                    request.timeout_ms = 100;
                    let serial = hex(&self.read_control(request, 12)?);
                    let reply = self.read_control(Request::new(0xc0, 0xb0, 0), 12)?;
                    Ok(Identity {
                        serial,
                        bootstub: reply.get(4..8) == Some(&[0xde, 0xad, 0xd0, 0x0d]),
                        version: 0,
                    })
                }
                result => result,
            }
        })();
        let identity = match result {
            Err(error) if error.retryable() => return Ok(None),
            result => result?,
        };
        if serial.is_some_and(|serial| serial != identity.serial) {
            return Ok(None);
        }
        if !ignore_version && identity.version != 2 {
            return Err(Error::Version(identity.version));
        }
        Ok(Some(identity))
    }

    pub fn read_control(&mut self, request: Request, length: usize) -> Result<Vec<u8>, Error> {
        let length_u16 = u16::try_from(length)
            .map_err(|_| Error::Invalid("SPI control read length exceeds uint16"))?;
        let mut data = vec![request.request];
        data.extend(request.value.to_le_bytes());
        data.extend(request.index.to_le_bytes());
        data.extend(length_u16.to_le_bytes());
        self.transfer(0, &data, request.timeout_ms, length, false)
    }
    pub fn write_control(&mut self, request: Request) -> Result<Vec<u8>, Error> {
        let mut data = vec![request.request];
        data.extend(request.value.to_le_bytes());
        data.extend(request.index.to_le_bytes());
        data.extend([0, 0]);
        self.transfer(
            0,
            &data,
            request.timeout_ms,
            1000,
            request.expect_disconnect,
        )
    }
    pub fn write_bulk(&mut self, endpoint: u8, data: &[u8], timeout: u32) -> Result<usize, Error> {
        for chunk in data.chunks(1984) {
            self.transfer(endpoint, chunk, timeout, 1000, false)?;
        }
        Ok(data.len())
    }
    pub fn read_bulk(
        &mut self,
        endpoint: u8,
        length: usize,
        timeout: u32,
    ) -> Result<Vec<u8>, Error> {
        let mut result = Vec::new();
        for _ in 0..length.div_ceil(1984) {
            let data = self.transfer(endpoint, &[], timeout, 1984, false)?;
            let received = data.len();
            result.extend(data);
            if received < 1984 {
                break;
            }
        }
        Ok(result)
    }
}
pub fn hex(data: &[u8]) -> String {
    data.iter().map(|byte| format!("{byte:02x}")).collect()
}

impl<I: Io> Transport for PandaSpi<I> {
    type Error = Fault;
    fn control_read(&mut self, request: Request, length: usize) -> Result<Vec<u8>, Fault> {
        self.read_control(request, length)
            .map_err(Error::into_fault)
    }
    fn control_write(&mut self, request: Request, _: &[u8]) -> Result<(), Fault> {
        self.write_control(request)
            .map(|_| ())
            .map_err(Error::into_fault)
    }
    fn bulk_write(&mut self, endpoint: u8, data: &[u8], timeout_ms: u32) -> Result<(), Fault> {
        self.write_bulk(endpoint, data, timeout_ms)
            .map(|_| ())
            .map_err(Error::into_fault)
    }
}
impl<I: Io> Handle for PandaSpi<I> {
    fn close(&mut self) -> Result<(), Fault> {
        Ok(())
    }
}
