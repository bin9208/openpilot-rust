use crate::Error;
pub struct Reader<'a> {
    remaining: &'a [u8],
}
impl<'a> Reader<'a> {
    pub fn new(remaining: &'a [u8]) -> Self {
        Self { remaining }
    }
    fn bytes<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let (value, rest) = self
            .remaining
            .split_at_checked(N)
            .ok_or(Error::Protocol("truncated diagnostic structure"))?;
        self.remaining = rest;
        value
            .try_into()
            .map_err(|_| Error::Protocol("diagnostic field width"))
    }
    pub fn u8(&mut self) -> Result<u8, Error> {
        Ok(u8::from_le_bytes(self.bytes()?))
    }
    pub fn i8(&mut self) -> Result<i8, Error> {
        Ok(i8::from_le_bytes(self.bytes()?))
    }
    pub fn u16(&mut self) -> Result<u16, Error> {
        Ok(u16::from_le_bytes(self.bytes()?))
    }
    pub fn i16(&mut self) -> Result<i16, Error> {
        Ok(i16::from_le_bytes(self.bytes()?))
    }
    pub fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.bytes()?))
    }
    pub fn i32(&mut self) -> Result<i32, Error> {
        Ok(i32::from_le_bytes(self.bytes()?))
    }
    pub fn u64(&mut self) -> Result<u64, Error> {
        Ok(u64::from_le_bytes(self.bytes()?))
    }
    pub fn f32(&mut self) -> Result<f32, Error> {
        Ok(f32::from_le_bytes(self.bytes()?))
    }
    pub fn f64(&mut self) -> Result<f64, Error> {
        Ok(f64::from_le_bytes(self.bytes()?))
    }
}
