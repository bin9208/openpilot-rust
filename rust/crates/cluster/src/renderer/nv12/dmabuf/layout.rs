use super::{Error, Result};
#[derive(Clone, Copy, Debug)]
pub struct PackedLayout {
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) stride: i32,
}
impl PackedLayout {
    /// # Errors
    /// Rejects incomplete RGBA rows or dimensions outside the native range.
    pub fn new(stride: usize, bytes: usize) -> Result<Self> {
        if stride == 0 || !stride.is_multiple_of(4) || bytes == 0 || !bytes.is_multiple_of(stride) {
            return Err(Error::Layout);
        }
        Ok(Self {
            width: i32::try_from(stride / 4).map_err(|_| Error::Layout)?,
            height: i32::try_from(bytes / stride).map_err(|_| Error::Layout)?,
            stride: i32::try_from(stride).map_err(|_| Error::Layout)?,
        })
    }
    #[must_use]
    pub fn dimensions(self) -> (i32, i32) {
        (self.width, self.height)
    }
    pub(super) fn attributes(self, fd: i32) -> [i32; 13] {
        [
            0x3057,
            self.width,
            0x3056,
            self.height,
            0x3271,
            i32::from_le_bytes(*b"AB24"),
            0x3272,
            fd,
            0x3273,
            0,
            0x3274,
            self.stride,
            0x3038,
        ]
    }
}
