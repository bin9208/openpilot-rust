use crate::Error;

pub struct Cursor<'a> {
    data: &'a [u8],
    bit: usize,
}
macro_rules! scalar {
    ($name:ident, $ty:ty, $decode:ident) => {
        pub fn $name(&mut self) -> Result<$ty, Error> {
            Ok(<$ty>::$decode(self.bytes()?))
        }
    };
}
impl<'a> Cursor<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, bit: 0 }
    }
    pub fn skip(&mut self, count: usize) -> Result<(), Error> {
        let start = self.bit.div_ceil(8);
        let end = start.checked_add(count).ok_or(Error::Malformed("size"))?;
        self.data
            .get(start..end)
            .ok_or(Error::Malformed("truncated"))?;
        self.bit = end * 8;
        Ok(())
    }
    pub fn bytes<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let start = self.bit.div_ceil(8);
        self.skip(N)?;
        self.data[start..start + N]
            .try_into()
            .map_err(|_| Error::Malformed("truncated"))
    }
    pub fn bits(&mut self, count: usize) -> Result<u64, Error> {
        if count > 64 {
            return Err(Error::Malformed("bit width"));
        }
        let mut value = 0;
        for _ in 0..count {
            let byte = self
                .data
                .get(self.bit / 8)
                .ok_or(Error::Malformed("truncated bits"))?;
            value = (value << 1) | u64::from((byte >> (7 - self.bit % 8)) & 1);
            self.bit += 1;
        }
        Ok(value)
    }
    scalar!(u8, u8, from_le_bytes);
    scalar!(i8, i8, from_le_bytes);
    scalar!(u16, u16, from_le_bytes);
    scalar!(i16, i16, from_le_bytes);
    scalar!(u32, u32, from_le_bytes);
    scalar!(i32, i32, from_le_bytes);
    scalar!(u16be, u16, from_be_bytes);
    scalar!(i16be, i16, from_be_bytes);
    scalar!(u32be, u32, from_be_bytes);
    scalar!(i32be, i32, from_be_bytes);
    scalar!(f32, f32, from_le_bytes);
    scalar!(f64, f64, from_le_bytes);
}
