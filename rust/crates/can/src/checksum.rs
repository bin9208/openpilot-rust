use crate::{signal::Signal, volkswagen, Error};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Kind {
    Default,
    Counter,
    Honda,
    Toyota,
    Body,
    Volkswagen,
    VolkswagenGen2,
    Xor,
    Subaru,
    Chrysler,
    HyundaiFd,
    Giorgio,
    Tesla,
    Psa,
    VolkswagenMlb,
}

impl Kind {
    pub fn for_signal(dbc: &str, signal: &str) -> Self {
        let kind = if dbc.starts_with("honda_") || dbc.starts_with("acura_") {
            Self::Honda
        } else if dbc.starts_with("toyota_") || dbc.starts_with("lexus_") {
            Self::Toyota
        } else if dbc.starts_with("hyundai_canfd_generated") {
            Self::HyundaiFd
        } else if dbc.starts_with("vw_meb_2024") {
            Self::VolkswagenGen2
        } else if dbc.starts_with("vw_mqb")
            || dbc.starts_with("vw_mqbevo")
            || dbc.starts_with("vw_meb")
        {
            Self::Volkswagen
        } else if dbc.starts_with("vw_mlb") {
            Self::VolkswagenMlb
        } else if dbc.starts_with("vw_pq") {
            Self::Xor
        } else if dbc.starts_with("subaru_global_") {
            Self::Subaru
        } else if dbc.starts_with("chrysler_") {
            Self::Chrysler
        } else if dbc.starts_with("fca_giorgio") {
            Self::Giorgio
        } else if dbc.starts_with("comma_body") {
            Self::Body
        } else if dbc.starts_with("tesla_model3_party") {
            Self::Tesla
        } else if dbc.starts_with("psa_") {
            Self::Psa
        } else {
            Self::Default
        };
        if kind == Self::Tesla && signal.ends_with("Counter") {
            return Self::Counter;
        }
        if kind == Self::Tesla && signal.ends_with("Checksum") {
            return kind;
        }
        match signal {
            "CHECKSUM" => kind,
            "COUNTER" if kind != Self::Default => Self::Counter,
            _ => Self::Default,
        }
    }

    pub fn is_checksum(self) -> bool {
        !matches!(self, Self::Default | Self::Counter)
    }

    pub fn compute(self, address: u32, signal: &Signal, data: &mut [u8]) -> Result<u16, Error> {
        let last = data.len().saturating_sub(1);
        let value = match self {
            Self::Default | Self::Counter => return Err(Error::Checksum),
            Self::Honda => {
                let mut sum = 0u32;
                let mut addr = address;
                while addr != 0 {
                    sum += addr & 15;
                    addr >>= 4;
                }
                for (i, byte) in data.iter().enumerate() {
                    let byte = if i == last { byte >> 4 } else { *byte };
                    sum += u32::from((byte & 15) + (byte >> 4));
                }
                u16::try_from(
                    8u32.wrapping_sub(sum)
                        .wrapping_add(if address > 0x7ff { 3 } else { 0 })
                        & 15,
                )
                .map_err(|_| Error::Numeric)?
            }
            Self::Toyota | Self::Subaru => {
                let mut sum = if self == Self::Toyota {
                    u32::try_from(data.len()).map_err(|_| Error::Numeric)?
                } else {
                    0
                };
                let mut addr = address;
                while addr != 0 {
                    sum += addr & 255;
                    addr >>= 8;
                }
                let bytes = if self == Self::Toyota {
                    &data[..last]
                } else {
                    data.get(1..).unwrap_or_default()
                };
                sum += bytes.iter().map(|b| u32::from(*b)).sum::<u32>();
                u16::try_from(sum & 255).map_err(|_| Error::Numeric)?
            }
            Self::Tesla => {
                let sum = (address & 255)
                    + ((address >> 8) & 255)
                    + data
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| *i != signal.start_bit / 8)
                        .map(|(_, b)| u32::from(*b))
                        .sum::<u32>();
                u16::try_from(sum & 255).map_err(|_| Error::Numeric)?
            }
            Self::Psa => {
                let byte = data.get_mut(signal.start_bit / 8).ok_or(Error::Checksum)?;
                *byte &= if signal.start_bit % 8 >= 4 { 15 } else { 240 };
                let sum = data
                    .iter()
                    .map(|b| i32::from((b >> 4) + (b & 15)))
                    .sum::<i32>();
                let init = match address {
                    0x452 => 4,
                    0x38d => 7,
                    0x42d => 12,
                    _ => 11,
                };
                u16::try_from((init - sum) & 15).map_err(|_| Error::Numeric)?
            }
            Self::Body => u16::from(
                data[..last]
                    .iter()
                    .rev()
                    .fold(255, |crc, b| crc8(crc ^ b, 0xd5)),
            ),
            Self::Chrysler => {
                u16::from(data[..last].iter().fold(255, |crc, b| crc8(crc ^ b, 0x1d)) ^ 255)
            }
            Self::Giorgio => {
                let crc = data[..last].iter().fold(0, |crc, b| crc8(crc ^ b, 0x1d));
                u16::from(
                    crc ^ match address {
                        0xde => 0x10,
                        0x106 => 0xf6,
                        0x122 => 0xf1,
                        _ => 0x0a,
                    },
                )
            }
            Self::HyundaiFd => {
                let mut crc = 0;
                for byte in data
                    .iter()
                    .skip(2)
                    .copied()
                    .chain(address.to_le_bytes().into_iter().take(2))
                {
                    crc = crc16(crc, byte);
                }
                crc ^ match data.len() {
                    8 => 0x5f29,
                    16 => 0x041d,
                    24 => 0x819d,
                    32 => 0x9f5b,
                    _ => 0,
                }
            }
            Self::Xor => u16::from(
                data.iter()
                    .enumerate()
                    .filter(|(i, _)| *i != signal.start_bit / 8)
                    .fold(0, |crc, (_, b)| crc ^ b),
            ),
            Self::Volkswagen | Self::VolkswagenGen2 | Self::VolkswagenMlb => {
                return volkswagen::checksum(self, address, signal, data)
            }
        };
        Ok(value)
    }
}

pub(crate) fn crc8(mut value: u8, polynomial: u8) -> u8 {
    for _ in 0..8 {
        value = value.wrapping_shl(1) ^ if value & 128 != 0 { polynomial } else { 0 };
    }
    value
}

fn crc16(mut value: u16, byte: u8) -> u16 {
    value ^= u16::from(byte) << 8;
    for _ in 0..8 {
        value = value.wrapping_shl(1) ^ if value & 0x8000 != 0 { 0x1021 } else { 0 };
    }
    value
}
