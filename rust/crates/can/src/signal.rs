use crate::{checksum::Kind, Error};
use num_bigint::BigInt;
use num_traits::{One, ToPrimitive, Zero};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Signal {
    pub name: String,
    pub start_bit: usize,
    pub msb: usize,
    pub lsb: usize,
    pub size: usize,
    pub signed: bool,
    pub factor: f64,
    pub offset: f64,
    pub little_endian: bool,
    pub kind: Kind,
}

impl Signal {
    pub fn raw(&self, data: &[u8]) -> Result<BigInt, Error> {
        let mut value = BigInt::zero();
        let mut index = isize::try_from(self.msb / 8).map_err(|_| Error::Numeric)?;
        let mut bits = self.size;
        while let Ok(i) = usize::try_from(index) {
            let Some(byte) = data.get(i) else { break };
            if bits == 0 {
                break;
            }
            let lsb = if self.lsb / 8 == i { self.lsb } else { i * 8 };
            let msb = if self.msb / 8 == i {
                self.msb
            } else {
                (i + 1) * 8 - 1
            };
            let size = msb.checked_sub(lsb).ok_or(Error::Numeric)? + 1;
            let remaining = bits.checked_sub(size).ok_or(Error::Numeric)?;
            let mask = u16::try_from((1usize << size) - 1).map_err(|_| Error::Numeric)?;
            let part = (u16::from(*byte) >> (lsb - i * 8)) & mask;
            value |= BigInt::from(part) << remaining;
            bits = remaining;
            index += if self.little_endian { -1 } else { 1 };
        }
        Ok(value)
    }

    pub fn physical(&self, raw: &BigInt) -> Result<f64, Error> {
        let mut value = raw.clone();
        if self.signed {
            if self.size == 0 {
                return Err(Error::Numeric);
            }
            let sign = (&value >> (self.size - 1)) & BigInt::one();
            value -= sign << self.size;
        }
        Ok(value.to_f64().ok_or(Error::Numeric)? * self.factor + self.offset)
    }

    pub fn set(&self, data: &mut [u8], raw: &BigInt) -> Result<(), Error> {
        let mut value = raw.clone();
        if self.size < 64 {
            value &= (BigInt::one() << self.size) - 1;
        }
        let mut index = isize::try_from(self.lsb / 8).map_err(|_| Error::Numeric)?;
        let mut bits = self.size;
        while let Ok(i) = usize::try_from(index) {
            let Some(byte) = data.get_mut(i) else { break };
            if bits == 0 {
                break;
            }
            let shift = if self.lsb / 8 == i { self.lsb % 8 } else { 0 };
            let size = bits.min(8 - shift);
            let mask = u8::try_from(((1u16 << size) - 1) << shift).map_err(|_| Error::Numeric)?;
            let part = (&value & ((BigInt::one() << size) - BigInt::one()))
                .to_u8()
                .ok_or(Error::Numeric)?;
            *byte = (*byte & !mask) | (part << shift);
            bits -= size;
            value >>= size;
            index += if self.little_endian { 1 } else { -1 };
        }
        Ok(())
    }

    pub fn counter(&self) -> bool {
        self.kind == Kind::Counter || self.name == "COUNTER"
    }
}
