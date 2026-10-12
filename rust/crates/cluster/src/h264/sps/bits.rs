pub(super) type Result<T> = std::result::Result<T, &'static str>;
pub(super) struct Reader<'a> {
    pub data: &'a [u8],
    pub position: usize,
}
impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }
    pub fn bit(&mut self) -> Result<u64> {
        let byte = self
            .data
            .get(self.position / 8)
            .ok_or("SPS ended unexpectedly")?;
        let value = u64::from((byte >> (7 - self.position % 8)) & 1);
        self.position += 1;
        Ok(value)
    }
    pub fn bits(&mut self, count: usize) -> Result<u64> {
        let mut value = 0;
        for _ in 0..count {
            value = (value << 1) | self.bit()?;
        }
        Ok(value)
    }
    pub fn ue(&mut self) -> Result<u64> {
        let mut zeros = 0;
        while self.bit()? == 0 {
            zeros += 1;
            if zeros > 31 {
                return Err("SPS Exp-Golomb value is too large");
            }
        }
        let mut value = 1;
        for _ in 0..zeros {
            value = (value << 1) | self.bit()?;
        }
        Ok(value - 1)
    }
    pub fn se(&mut self) -> Result<i64> {
        let value = self.ue()?;
        let sign = if value & 1 == 0 { -1 } else { 1 };
        Ok(sign * i64::try_from(value.div_ceil(2)).map_err(|_| "SPS signed value overflow")?)
    }
}
#[derive(Default)]
pub(super) struct Writer {
    bits: Vec<bool>,
}
impl Writer {
    pub fn bit(&mut self, value: bool) {
        self.bits.push(value);
    }
    pub fn bits(&mut self, value: u64, count: usize) {
        for shift in (0..count).rev() {
            self.bit((value >> shift) & 1 != 0);
        }
    }
    pub fn ue(&mut self, value: u64) -> Result<()> {
        let code = value.checked_add(1).ok_or("SPS code number overflow")?;
        let count = usize::try_from(u64::BITS - code.leading_zeros())
            .map_err(|_| "SPS bit count overflow")?;
        for _ in 1..count {
            self.bit(false);
        }
        self.bits(code, count);
        Ok(())
    }
    pub fn copy(&mut self, data: &[u8], start: usize, end: usize) -> Result<()> {
        for bit in start..end {
            let value = data.get(bit / 8).ok_or("SPS copy exceeds buffer")?;
            self.bit((value >> (7 - bit % 8)) & 1 != 0);
        }
        Ok(())
    }
    pub fn bytes(self) -> Vec<u8> {
        let mut bytes = vec![0; self.bits.len().div_ceil(8)];
        for (index, bit) in self.bits.into_iter().enumerate() {
            if bit {
                bytes[index / 8] |= 1 << (7 - index % 8);
            }
        }
        bytes
    }
}
pub(super) fn unescape(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut zeros = 0;
    for &byte in data {
        if zeros >= 2 && byte == 3 {
            zeros = 0;
            continue;
        }
        out.push(byte);
        zeros = if byte == 0 { zeros + 1 } else { 0 };
    }
    out
}
pub(super) fn escape(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut zeros = 0;
    for &byte in data {
        if zeros >= 2 && byte <= 3 {
            out.push(3);
            zeros = 0;
        }
        out.push(byte);
        zeros = if byte == 0 { zeros + 1 } else { 0 };
    }
    out
}
pub(super) fn stop(data: &[u8]) -> Result<usize> {
    let bits = data.len().checked_mul(8).ok_or("SPS bit count overflow")?;
    (0..bits)
        .rev()
        .find(|&position| (data[position / 8] >> (7 - position % 8)) & 1 != 0)
        .ok_or("SPS RBSP stop bit not found")
}
