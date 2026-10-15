//! Scalar storage and arithmetic for the lowered IR's native reference executor.
use super::parse::width;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scalar {
    pub dtype: u32,
    pub bits: u64,
    literal: Option<f64>,
}
impl Scalar {
    pub fn read(dtype: u32, bytes: &[u8]) -> Result<Self, String> {
        let size = width(dtype)?;
        if bytes.len() != size {
            return Err("scalar storage width mismatch".into());
        }
        let mut raw = [0; 8];
        raw[..size].copy_from_slice(bytes);
        Ok(Self {
            dtype,
            bits: u64::from_le_bytes(raw),
            literal: None,
        })
    }
    pub fn bytes(self) -> Result<Vec<u8>, String> {
        if let Some(value) = self.literal {
            return Self::from_float(self.dtype, value)?.bytes();
        }
        Ok(self.bits.to_le_bytes()[..width(self.dtype)?].to_vec())
    }
    pub fn floating(self) -> bool {
        matches!(self.dtype, 1 | 10 | 11 | 16)
    }
    pub fn integer(self) -> Result<i128, String> {
        let bits = width(self.dtype)? * 8;
        Ok(if matches!(self.dtype, 3 | 5 | 6 | 7) {
            ((self.bits as i128) << (128 - bits)) >> (128 - bits)
        } else {
            self.bits as i128
        })
    }
    pub fn float(self) -> Result<f64, String> {
        if let Some(value) = self.literal {
            return Ok(value);
        }
        Ok(match self.dtype {
            1 => f32::from_bits(self.bits as u32) as f64,
            11 => f64::from_bits(self.bits),
            16 => f32::from_bits((self.bits as u32) << 16) as f64,
            10 => {
                let h = self.bits as u16;
                let sign = if h & 0x8000 != 0 { -1. } else { 1. };
                let exp = (h >> 10) & 31;
                let mant = h & 1023;
                if exp == 31 {
                    if mant == 0 {
                        sign * f64::INFINITY
                    } else {
                        f64::NAN
                    }
                } else if exp == 0 {
                    sign * (mant as f64) * 2f64.powi(-24)
                } else {
                    sign * (1024 + mant) as f64 * 2f64.powi(exp as i32 - 25)
                }
            }
            _ => self.integer()? as f64,
        })
    }
    pub fn truth(self) -> Result<bool, String> {
        Ok(if self.floating() {
            self.float()? != 0.
        } else {
            self.integer()? != 0
        })
    }
    pub fn from_integer(dtype: u32, value: i128) -> Result<Self, String> {
        let bits = if matches!(dtype, 1 | 10 | 11 | 16) {
            return Self::from_float(dtype, value as f64);
        } else if dtype == 9 {
            u64::from(value != 0)
        } else {
            let size = width(dtype)? * 8;
            if size == 64 {
                value as u64
            } else {
                (value as u64) & ((1u64 << size) - 1)
            }
        };
        Ok(Self {
            dtype,
            bits,
            literal: None,
        })
    }
    pub fn from_float(dtype: u32, value: f64) -> Result<Self, String> {
        let bits = match dtype {
            1 => (value as f32).to_bits() as u64,
            11 => value.to_bits(),
            10 => half(value) as u64,
            16 => {
                let value = value as f32;
                let bits = value.to_bits();
                if !value.is_finite() {
                    (bits >> 16) as u64
                } else {
                    (bits.wrapping_add(0x7fff + ((bits >> 16) & 1)) >> 16) as u64
                }
            }
            9 => u64::from(value != 0.),
            _ => {
                if !value.is_finite() || value.abs() >= 2f64.powi(127) {
                    return Err("nonfinite or excessive float-to-integer cast".into());
                }
                return Self::from_integer(dtype, value.trunc() as i128);
            }
        };
        Ok(Self {
            dtype,
            bits,
            literal: None,
        })
    }
    /// PythonRenderer retains typed CONST args at Python float precision until
    /// an ALU instruction or a storage write rounds them. The IR retains both
    /// the nominal dtype and the original constant for independent replay.
    pub fn floating_literal(dtype: u32, value: f64) -> Result<Self, String> {
        let mut scalar = Self::from_float(dtype, value)?;
        if !scalar.floating() {
            return Err("floating literal requires floating dtype".into());
        }
        scalar.literal = Some(value);
        Ok(scalar)
    }
    pub fn cast(self, dtype: u32) -> Result<Self, String> {
        if dtype == self.dtype {
            return Ok(self);
        }
        if self.floating() {
            Self::from_float(dtype, self.float()?)
        } else {
            Self::from_integer(dtype, self.integer()?)
        }
    }
}
fn rounded(value: u64, shift: u32) -> u64 {
    if shift > 63 {
        return 0;
    }
    let base = value >> shift;
    let mask = (1u64 << shift) - 1;
    let remainder = value & mask;
    let midpoint = 1u64 << (shift - 1);
    base + u64::from(remainder > midpoint || (remainder == midpoint && base & 1 != 0))
}
fn half(value: f64) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 48) & 0x8000) as u16;
    let exp = ((bits >> 52) & 2047) as i32;
    let mant = bits & ((1u64 << 52) - 1);
    if exp == 2047 {
        return sign | if mant == 0 { 0x7c00 } else { 0x7e00 };
    }
    if exp == 0 {
        return sign;
    }
    let target = exp - 1023 + 15;
    if target > 30 {
        return sign | 0x7c00;
    }
    if target < -10 {
        return sign;
    }
    let significand = mant | (1u64 << 52);
    if target <= 0 {
        return sign | rounded(significand, (43 - target) as u32) as u16;
    }
    sign | ((((target - 1) as u16) << 10) + rounded(significand, 42) as u16)
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Binary {
    Add,
    Sub,
    Mul,
    DivTrunc,
    Maximum,
    Minimum,
}
pub fn binary(kind: Binary, dtype: u32, a: Scalar, b: Scalar) -> Result<Scalar, String> {
    let (a, b) = (a.cast(dtype)?, b.cast(dtype)?);
    if a.floating() {
        let (x, y) = (a.float()?, b.float()?);
        return Scalar::from_float(
            dtype,
            match kind {
                Binary::Add => x + y,
                Binary::Sub => x - y,
                Binary::Mul => x * y,
                Binary::DivTrunc => return Err("floating truncating divide in scalar IR".into()),
                Binary::Maximum => {
                    if y > x {
                        y
                    } else {
                        x
                    }
                }
                Binary::Minimum => {
                    if y < x {
                        y
                    } else {
                        x
                    }
                }
            },
        );
    }
    let (x, y) = (a.integer()?, b.integer()?);
    Scalar::from_integer(
        dtype,
        match kind {
            Binary::Add => x.wrapping_add(y),
            Binary::Sub => x.wrapping_sub(y),
            Binary::Mul => x.wrapping_mul(y),
            Binary::DivTrunc => {
                if y == 0 {
                    0
                } else {
                    x / y
                }
            }
            Binary::Maximum => x.max(y),
            Binary::Minimum => x.min(y),
        },
    )
}
pub fn promote(a: u32, b: u32) -> Result<u32, String> {
    const IDS: [u32; 13] = [1, 2, 3, 4, 5, 6, 7, 9, 10, 11, 12, 13, 16];
    // Exact least_upper_dtype table from the pinned original dtype lattice.
    const TABLE: [[u32; 13]; 13] = [
        [1, 1, 1, 1, 1, 1, 1, 1, 1, 11, 1, 1, 1],
        [1, 2, 5, 4, 5, 6, 7, 2, 10, 11, 12, 13, 16],
        [1, 5, 3, 6, 5, 6, 7, 3, 10, 11, 7, 13, 16],
        [1, 4, 6, 4, 6, 6, 7, 4, 10, 11, 12, 13, 16],
        [1, 5, 5, 6, 5, 6, 7, 5, 10, 11, 7, 13, 16],
        [1, 6, 6, 6, 6, 6, 7, 6, 10, 11, 7, 13, 16],
        [1, 7, 7, 7, 7, 7, 7, 7, 10, 11, 7, 13, 16],
        [1, 2, 3, 4, 5, 6, 7, 9, 10, 11, 12, 13, 16],
        [1, 10, 10, 10, 10, 10, 10, 10, 10, 11, 10, 10, 1],
        [11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11],
        [1, 12, 7, 12, 7, 7, 7, 12, 10, 11, 12, 13, 16],
        [1, 13, 13, 13, 13, 13, 13, 13, 10, 11, 13, 13, 16],
        [1, 16, 16, 16, 16, 16, 16, 16, 1, 11, 16, 16, 16],
    ];
    let i = IDS
        .iter()
        .position(|&v| v == a)
        .ok_or("unsupported scalar dtype")?;
    let j = IDS
        .iter()
        .position(|&v| v == b)
        .ok_or("unsupported scalar dtype")?;
    Ok(TABLE[i][j])
}
