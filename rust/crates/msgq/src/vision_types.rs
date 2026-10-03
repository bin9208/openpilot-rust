use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisionStream {
    Road,
    Driver,
    WideRoad,
    Map,
}

impl VisionStream {
    pub(crate) const fn native(self) -> i32 {
        match self {
            Self::Road => 0,
            Self::Driver => 1,
            Self::WideRoad => 2,
            Self::Map => 3,
        }
    }

    pub(crate) fn from_native(value: i32) -> Result<Self, Error> {
        match value {
            0 => Ok(Self::Road),
            1 => Ok(Self::Driver),
            2 => Ok(Self::WideRoad),
            3 => Ok(Self::Map),
            _ => Err(Error::Corrupt("invalid VisionIPC stream")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisionLayout {
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub uv_offset: usize,
    pub len: usize,
}

impl VisionLayout {
    pub(crate) fn validate(self, mapped_length: usize) -> Result<(), Error> {
        let y_length = self.stride.checked_mul(self.height);
        let uv_length = self.stride.checked_mul(self.height / 2);
        if self.width == 0
            || self.height == 0
            || !self.width.is_multiple_of(2)
            || !self.height.is_multiple_of(2)
            || self.stride < self.width
            || y_length.is_none_or(|length| self.uv_offset < length)
            || self.uv_offset > self.len
            || uv_length.is_none_or(|length| length > self.len.saturating_sub(self.uv_offset))
            || self
                .len
                .checked_add(8)
                .is_none_or(|length| length > mapped_length)
        {
            return Err(Error::Invalid("invalid VisionIPC NV12 buffer layout"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct VisionMetadata {
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub uv_offset: usize,
    pub len: usize,
    pub frame_id: u32,
    pub timestamp_sof: u64,
    pub timestamp_eof: u64,
    pub valid: bool,
    pub received: bool,
    pub index: usize,
    pub fd: i32,
}
