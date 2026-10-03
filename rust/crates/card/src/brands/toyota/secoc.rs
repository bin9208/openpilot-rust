use super::Error;
use aes::{Aes128, Aes192, Aes256};
use cmac::{Cmac, Mac};
use openpilot_can::Frame;

#[derive(Clone)]
pub enum Key {
    Aes128([u8; 16]),
    Aes192([u8; 24]),
    Aes256([u8; 32]),
}
impl Default for Key {
    fn default() -> Self {
        Self::Aes256([b'0'; 32])
    }
}
impl Key {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        match bytes.len() {
            16 => Ok(Self::Aes128(
                bytes.try_into().map_err(|_| Error::KeyLength)?,
            )),
            24 => Ok(Self::Aes192(
                bytes.try_into().map_err(|_| Error::KeyLength)?,
            )),
            32 => Ok(Self::Aes256(
                bytes.try_into().map_err(|_| Error::KeyLength)?,
            )),
            _ => Err(Error::KeyLength),
        }
    }
    fn tag(&self, data: &[u8]) -> Result<[u8; 16], Error> {
        match self {
            Self::Aes128(key) => {
                let mut mac = Cmac::<Aes128>::new_from_slice(key).map_err(|_| Error::KeyLength)?;
                mac.update(data);
                Ok(mac.finalize().into_bytes().into())
            }
            Self::Aes192(key) => {
                let mut mac = Cmac::<Aes192>::new_from_slice(key).map_err(|_| Error::KeyLength)?;
                mac.update(data);
                Ok(mac.finalize().into_bytes().into())
            }
            Self::Aes256(key) => {
                let mut mac = Cmac::<Aes256>::new_from_slice(key).map_err(|_| Error::KeyLength)?;
                mac.update(data);
                Ok(mac.finalize().into_bytes().into())
            }
        }
    }
    pub fn sync_mac(&self, trip: u16, reset: u32) -> Result<u32, Error> {
        if reset > 0xfffff {
            return Err(Error::Numeric);
        }
        let reset = (reset << 12).to_be_bytes();
        let mut data = Vec::from([0, 0xf]);
        data.extend_from_slice(&trip.to_be_bytes());
        data.extend_from_slice(&reset[..3]);
        let tag = self.tag(&data)?;
        Ok(u32::from_be_bytes(tag[..4].try_into().map_err(|_| Error::Numeric)?) >> 4)
    }
    pub fn authenticate(&self, freshness: Freshness, mut frame: Frame) -> Result<Frame, Error> {
        if freshness.reset > 0xfffff {
            return Err(Error::Numeric);
        }
        let address = u16::try_from(frame.address).map_err(|_| Error::Numeric)?;
        let reset_flag = freshness.reset & 3;
        let message = u32::try_from(freshness.message & 0xff).map_err(|_| Error::Numeric)?;
        let tail = (freshness.reset << 12) | (message << 4) | (reset_flag << 2);
        let mut data = Vec::from(address.to_be_bytes());
        data.extend_from_slice(&frame.data[..frame.data.len().min(4)]);
        data.extend_from_slice(&freshness.trip.to_be_bytes());
        data.extend_from_slice(&tail.to_be_bytes());
        let tag = self.tag(&data)?;
        let mac = u32::from_be_bytes(tag[..4].try_into().map_err(|_| Error::Numeric)?) >> 4;
        let flags = ((message & 3) << 2) | reset_flag;
        frame.data.truncate(4);
        frame
            .data
            .extend_from_slice(&((flags << 28) | mac).to_be_bytes());
        Ok(frame)
    }
}
pub struct Freshness {
    pub trip: u16,
    pub reset: u32,
    pub message: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmac_matches_rfc4493_empty_message_vector() {
        let key = Key::Aes128([
            0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6, 0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf,
            0x4f, 0x3c,
        ]);
        let tag = key.tag(&[]).unwrap();
        assert_eq!(
            tag,
            [
                0xbb, 0x1d, 0x69, 0x29, 0xe9, 0x59, 0x37, 0x28, 0x7f, 0xa3, 0x7d, 0x12, 0x9b, 0x75,
                0x67, 0x46
            ]
        );
    }

    #[test]
    fn unsupported_key_lengths_return_typed_error() {
        for length in [0, 15, 17, 23, 25, 31, 33] {
            let key = vec![0; length];
            let result = Key::parse(&key);
            assert!(matches!(result, Err(Error::KeyLength)));
        }
    }

    #[test]
    fn overflowing_reset_counter_returns_typed_error() {
        let key = Key::default();
        let result = key.sync_mac(0, 0x100000);
        assert!(matches!(result, Err(Error::Numeric)));
    }
}
