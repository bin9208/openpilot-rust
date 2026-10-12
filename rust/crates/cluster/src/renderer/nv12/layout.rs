use super::super::{Error, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Nv12Layout {
    pub width: i32,
    pub height: i32,
    pub stride: i32,
    pub y_scanlines: i32,
    pub uv_scanlines: i32,
    pub uv_offset: usize,
    pub byte_count: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectPacking {
    pub width: i32,
    pub height: i32,
    pub uv_y: i32,
    pub y_y: i32,
}
fn aligned(value: usize, alignment: usize) -> Result<usize> {
    value
        .checked_add(alignment - 1)
        .map(|value| value / alignment * alignment)
        .ok_or(Error::Contract("Venus layout overflow"))
}
fn converted(value: usize) -> Result<i32> {
    i32::try_from(value).map_err(|_| Error::Contract("Venus layout dimension overflow"))
}
impl Nv12Layout {
    /// # Errors
    /// Rejects inconsistent dimensions, offsets, allocation sizes, and overflow.
    pub fn validate(self) -> Result<()> {
        let invalid = || Error::Contract("NV12 render target layout is invalid");
        if self.width <= 0 || self.height <= 0 || self.stride <= 0 || self.byte_count == 0 {
            return Err(invalid());
        }
        if self.stride < self.width
            || self.y_scanlines < self.height
            || self.uv_scanlines < self.height / 2 + self.height % 2
        {
            return Err(Error::Contract(
                "NV12 render target layout is smaller than the rendered frame",
            ));
        }
        let stride = usize::try_from(self.stride).map_err(|_| invalid())?;
        let y = usize::try_from(self.y_scanlines).map_err(|_| invalid())?;
        let uv = usize::try_from(self.uv_scanlines).map_err(|_| invalid())?;
        let y_bytes = stride
            .checked_mul(y)
            .ok_or(Error::Contract("NV12 byte layout overflow"))?;
        let end = self
            .uv_offset
            .checked_add(
                stride
                    .checked_mul(uv)
                    .ok_or(Error::Contract("NV12 byte layout overflow"))?,
            )
            .ok_or(Error::Contract("NV12 byte layout overflow"))?;
        if self.uv_offset < y_bytes
            || self.byte_count < end
            || isize::try_from(self.byte_count).is_err()
        {
            return Err(Error::Contract(
                "NV12 render target byte layout is inconsistent",
            ));
        }
        if self.stride % 4 == 0
            && self.byte_count.is_multiple_of(stride)
            && self.uv_offset.is_multiple_of(stride)
            && i32::try_from(self.byte_count / stride).is_err()
        {
            return Err(Error::Contract("NV12 packed target height overflow"));
        }
        Ok(())
    }
    /// # Errors
    /// Rejects nonpositive dimensions or an overflowing native Venus layout.
    pub fn venus(width: i32, height: i32) -> Result<Self> {
        if width <= 0 || height <= 0 {
            return Err(Error::Contract("Venus dimensions must be positive"));
        }
        let w = usize::try_from(width).map_err(|_| Error::Contract("Venus width overflow"))?;
        let h = usize::try_from(height).map_err(|_| Error::Contract("Venus height overflow"))?;
        let stride = aligned(w, 128)?;
        let y = aligned(h, 32)?;
        let uv = aligned(h.div_ceil(2), 16)?;
        let offset = stride
            .checked_mul(y)
            .ok_or(Error::Contract("Venus offset overflow"))?;
        let padding = 16_384.max(
            stride
                .checked_mul(8)
                .ok_or(Error::Contract("Venus padding overflow"))?,
        );
        let body = offset
            .checked_add(
                stride
                    .checked_mul(uv)
                    .ok_or(Error::Contract("Venus UV size overflow"))?,
            )
            .and_then(|value| value.checked_add(4096))
            .and_then(|value| value.checked_add(padding))
            .ok_or(Error::Contract("Venus buffer overflow"))?;
        let extra = aligned(w, 512)?
            .checked_mul(512)
            .ok_or(Error::Contract("Venus extradata overflow"))?;
        let byte_count = aligned(
            aligned(body, 4096)?
                .checked_add(extra)
                .ok_or(Error::Contract("Venus buffer overflow"))?,
            4096,
        )?;
        let value = Self {
            width,
            height,
            stride: converted(stride)?,
            y_scanlines: converted(y)?,
            uv_scanlines: converted(uv)?,
            uv_offset: offset,
            byte_count,
        };
        value.validate()?;
        Ok(value)
    }
    #[must_use]
    pub fn direct_packing(self) -> Option<DirectPacking> {
        self.validate().ok()?;
        let stride = usize::try_from(self.stride).ok()?;
        if self.stride % 4 != 0
            || !self.byte_count.is_multiple_of(stride)
            || !self.uv_offset.is_multiple_of(stride)
        {
            return None;
        }
        let height = i32::try_from(self.byte_count / stride).ok()?;
        let uv_y = (height - self.y_scanlines - self.uv_scanlines).max(0);
        Some(DirectPacking {
            width: self.stride / 4,
            height,
            uv_y,
            y_y: uv_y + self.uv_scanlines,
        })
    }
    pub(super) fn copy_planes(
        self,
        mut buffer: Vec<u8>,
        planes: [&[u8]; 2],
        full_stride: bool,
    ) -> Result<Vec<u8>> {
        let stride =
            usize::try_from(self.stride).map_err(|_| Error::Contract("NV12 stride invalid"))?;
        if buffer.len() != self.byte_count {
            buffer.resize(self.byte_count, 128);
            buffer[..self.uv_offset].fill(16);
            buffer[self.uv_offset..].fill(128);
        }
        if full_stride {
            let y_bytes = stride
                * usize::try_from(self.y_scanlines)
                    .map_err(|_| Error::Contract("NV12 y height invalid"))?;
            let uv_bytes = stride
                * usize::try_from(self.uv_scanlines)
                    .map_err(|_| Error::Contract("NV12 UV height invalid"))?;
            buffer[..y_bytes].copy_from_slice(planes[0]);
            buffer[self.uv_offset..self.uv_offset + uv_bytes].copy_from_slice(planes[1]);
        } else {
            let width =
                usize::try_from(self.width).map_err(|_| Error::Contract("NV12 width invalid"))?;
            let height =
                usize::try_from(self.height).map_err(|_| Error::Contract("NV12 height invalid"))?;
            let packed_stride = width.div_ceil(4) * 4;
            for (plane, offset, rows) in [
                (planes[0], 0, height),
                (planes[1], self.uv_offset, height.div_ceil(2)),
            ] {
                for row in 0..rows {
                    buffer[offset + row * stride..offset + row * stride + width]
                        .copy_from_slice(&plane[row * packed_stride..row * packed_stride + width]);
                }
            }
        }
        Ok(buffer)
    }
}
