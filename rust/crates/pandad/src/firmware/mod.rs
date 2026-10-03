pub mod client;
pub mod dfu_spi;
pub mod dfu_usb;
#[cfg(feature = "native-skip-miri")]
pub mod native_environment;
#[cfg(feature = "native-skip-miri")]
pub mod native_spi;
pub mod spi;
#[cfg(feature = "native-skip-miri")]
pub mod usb;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum Mcu {
    F4,
    H7,
}

impl Mcu {
    pub fn sectors(self) -> &'static [usize] {
        match self {
            Self::F4 => &[
                0x4000, 0x4000, 0x4000, 0x4000, 0x10000, 0x20000, 0x20000, 0x20000, 0x20000,
                0x20000, 0x20000, 0x20000, 0x20000, 0x20000, 0x20000, 0x20000,
            ],
            Self::H7 => &[0x20000; 7],
        }
    }

    pub fn block_size(self) -> usize {
        match self {
            Self::F4 => 0x800,
            Self::H7 => 0x400,
        }
    }

    pub fn app_filename(self) -> &'static str {
        match self {
            Self::F4 => "panda.bin.signed",
            Self::H7 => "panda_h7.bin.signed",
        }
    }

    pub fn bootstub_filename(self) -> &'static str {
        match self {
            Self::F4 => "bootstub.panda.bin",
            Self::H7 => "bootstub.panda_h7.bin",
        }
    }

    pub fn sector_address(self, index: usize) -> u32 {
        0x0800_0000 + self.sectors().iter().take(index).sum::<usize>() as u32
    }

    pub fn from_hardware(hardware: &[u8], assume_f4: bool) -> Option<Self> {
        match hardware {
            [1 | 2 | 3 | 5 | 6] => Some(Self::F4),
            [7..=10] => Some(Self::H7),
            _ if assume_f4 => Some(Self::F4),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Request {
    pub kind: u8,
    pub request: u8,
    pub value: u16,
    pub index: u16,
    pub timeout_ms: u32,
    pub expect_disconnect: bool,
}

impl Request {
    pub fn new(kind: u8, request: u8, value: u16) -> Self {
        Self {
            kind,
            request,
            value,
            index: 0,
            timeout_ms: 15_000,
            expect_disconnect: false,
        }
    }
}

pub trait Transport {
    type Error;
    fn control_read(&mut self, request: Request, length: usize) -> Result<Vec<u8>, Self::Error>;
    fn control_write(&mut self, request: Request, data: &[u8]) -> Result<(), Self::Error>;
    fn bulk_write(&mut self, endpoint: u8, data: &[u8], timeout_ms: u32)
        -> Result<(), Self::Error>;
}

#[derive(Debug, thiserror::Error)]
pub enum Error<E> {
    #[error("Panda firmware transport failed: {0}")]
    Transport(E),
    #[error("Panda flasher is missing")]
    FlasherMissing,
    #[error("Binary too small? No sector to erase.")]
    NoSector,
    #[error("Binary too large! Risk of overwriting provisioning chunk.")]
    ProvisioningSector,
    #[error("Panda DFU status is shorter than the required fields")]
    ShortStatus,
    #[error("Panda DFU USB cannot program an empty binary")]
    EmptyBinary,
    #[error("Panda DFU block number exceeds uint16")]
    BlockNumber,
}

pub fn flash_static<T: Transport>(
    transport: &mut T,
    code: &[u8],
    mcu: Mcu,
) -> Result<(), Error<T::Error>> {
    flash_static_with_log(transport, code, mcu, |_| Ok(()))
}

pub fn flash_static_with_log<T: Transport>(
    transport: &mut T,
    code: &[u8],
    mcu: Mcu,
    mut log: impl FnMut(String) -> Result<(), T::Error>,
) -> Result<(), Error<T::Error>> {
    let reply = transport
        .control_read(Request::new(0xc0, 0xb0, 0), 12)
        .map_err(Error::Transport)?;
    if reply.get(4..8) != Some(&[0xde, 0xad, 0xd0, 0x0d]) {
        return Err(Error::FlasherMissing);
    }
    let mut cumulative = 0;
    let last_sector = mcu
        .sectors()
        .iter()
        .skip(1)
        .position(|size| {
            cumulative += size;
            cumulative > code.len()
        })
        .map(|index| index + 1)
        .ok_or(Error::NoSector)?;
    if last_sector >= 7 {
        return Err(Error::ProvisioningSector);
    }
    log("flash: unlocking".into()).map_err(Error::Transport)?;
    transport
        .control_write(Request::new(0xc0, 0xb1, 0), &[])
        .map_err(Error::Transport)?;
    log(format!("flash: erasing sectors 1 - {last_sector}")).map_err(Error::Transport)?;
    for sector in 1..=last_sector {
        transport
            .control_write(Request::new(0xc0, 0xb2, sector as u16), &[])
            .map_err(Error::Transport)?;
    }
    log("flash: flashing".into()).map_err(Error::Transport)?;
    for chunk in code.chunks(16) {
        transport
            .bulk_write(2, chunk, 15_000)
            .map_err(Error::Transport)?;
    }
    log("flash: resetting".into()).map_err(Error::Transport)?;
    let mut reset = Request::new(0xc0, 0xd8, 0);
    reset.expect_disconnect = true;
    let _ = transport.control_write(reset, &[]);
    Ok(())
}

#[derive(Debug, thiserror::Error)]
#[error("invalid hexadecimal Panda UID")]
pub struct InvalidUid;

pub fn dfu_serial(serial: &str, mcu: Mcu) -> Result<Option<String>, InvalidUid> {
    if serial == "none" {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    let mut characters = serial.bytes();
    while let Some(high) = characters.next() {
        if high.is_ascii_whitespace() {
            continue;
        }
        let high = char::from(high).to_digit(16).ok_or(InvalidUid)?;
        let low = characters
            .next()
            .and_then(|byte| char::from(byte).to_digit(16))
            .ok_or(InvalidUid)?;
        bytes.push((high * 16 + low) as u8);
    }
    if bytes.len() != 12 {
        return Ok(None);
    }
    let words: Vec<_> = bytes
        .chunks_exact(2)
        .map(|bytes| u32::from(u16::from_le_bytes([bytes[0], bytes[1]])))
        .collect();
    let first = words[1] + words[5];
    let second = words[0] + words[4] + if mcu == Mcu::F4 { 10 } else { 0 };
    if first > 0xffff || second > 0xffff {
        return Ok(None);
    }
    Ok(Some(format!("{first:04X}{second:04X}{:04X}", words[3])))
}
