#[derive(Debug, Clone, Copy)]
pub struct Layout {
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub uv_offset: usize,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("invalid NV12 dimensions or stride")]
    Dimensions,
    #[error("NV12 plane size exceeds address space")]
    Overflow,
    #[error("short NV12 Y plane: {actual} < {required}")]
    ShortY { actual: usize, required: usize },
    #[error("NV12 width and height must be even")]
    OddDimensions,
    #[error("NV12 UV plane overlaps the visible Y plane")]
    Overlap,
    #[error("short NV12 UV plane: {actual} < {required}")]
    ShortUv { actual: usize, required: usize },
}

pub struct Frame<'a> {
    data: &'a [u8],
    layout: Layout,
    y_size: usize,
}

impl<'a> Frame<'a> {
    pub const fn layout(&self) -> Layout {
        self.layout
    }

    pub fn new(data: &'a [u8], layout: Layout) -> Result<Self, Error> {
        if layout.width == 0 || layout.height == 0 || layout.stride < layout.width {
            return Err(Error::Dimensions);
        }
        let y_size = layout
            .stride
            .checked_mul(layout.height)
            .ok_or(Error::Overflow)?;
        if data.len() < y_size {
            return Err(Error::ShortY {
                actual: data.len(),
                required: y_size,
            });
        }
        Ok(Self {
            data,
            layout,
            y_size,
        })
    }

    pub fn y_rows(&self) -> impl Iterator<Item = &'a [u8]> + '_ {
        self.data[..self.y_size]
            .chunks_exact(self.layout.stride)
            .map(|row| &row[..self.layout.width])
    }

    pub fn center_square(&self) -> Result<Vec<u8>, Error> {
        let size = self.layout.width.min(self.layout.height);
        let start_x = (self.layout.width - size) / 2;
        let start_y = (self.layout.height - size) / 2;
        let mut output = Vec::with_capacity(size.checked_mul(size).ok_or(Error::Overflow)?);
        for row in self.y_rows().skip(start_y).take(size) {
            output.extend_from_slice(&row[start_x..start_x + size]);
        }
        Ok(output)
    }

    fn validate_uv(&self) -> Result<usize, Error> {
        let layout = self.layout;
        if !layout.width.is_multiple_of(2) || !layout.height.is_multiple_of(2) {
            return Err(Error::OddDimensions);
        }
        if layout.uv_offset < self.y_size {
            return Err(Error::Overlap);
        }
        let uv_size = layout
            .stride
            .checked_mul(layout.height / 2)
            .ok_or(Error::Overflow)?;
        let uv_end = layout
            .uv_offset
            .checked_add(uv_size)
            .ok_or(Error::Overflow)?;
        if self.data.len() < uv_end {
            return Err(Error::ShortUv {
                actual: self.data.len(),
                required: uv_end,
            });
        }
        Ok(uv_end)
    }

    pub fn pack(&self) -> Result<Vec<u8>, Error> {
        let layout = self.layout;
        let uv_end = self.validate_uv()?;
        let rows = layout
            .height
            .checked_add(layout.height / 2)
            .ok_or(Error::Overflow)?;
        let capacity = rows.checked_mul(layout.width).ok_or(Error::Overflow)?;
        let mut output = Vec::with_capacity(capacity);
        for row in self.y_rows() {
            output.extend_from_slice(row);
        }
        for row in self.data[layout.uv_offset..uv_end].chunks_exact(layout.stride) {
            output.extend_from_slice(&row[..layout.width]);
        }
        Ok(output)
    }

    /// Packs an even-aligned [x, y, width, height] region as Y then NV12 UV.
    pub fn crop_nv12(&self, bounds: [usize; 4]) -> Result<Vec<u8>, Error> {
        // Preserve full-frame validation, including unselected UV rows/padding.
        let uv_end = self.validate_uv()?;
        let layout = self.layout;
        let [x, y, width, height] = bounds;
        let end_x = x.checked_add(width).ok_or(Error::Overflow)?;
        let end_y = y.checked_add(height).ok_or(Error::Overflow)?;
        if width == 0
            || height == 0
            || end_x > layout.width
            || end_y > layout.height
            || bounds.into_iter().any(|value| !value.is_multiple_of(2))
        {
            return Err(Error::Dimensions);
        }
        let rows = height.checked_add(height / 2).ok_or(Error::Overflow)?;
        let capacity = rows.checked_mul(width).ok_or(Error::Overflow)?;
        let mut output = Vec::with_capacity(capacity);
        for row in self.y_rows().skip(y).take(height) {
            output.extend_from_slice(&row[x..end_x]);
        }
        for row in self.data[layout.uv_offset..uv_end]
            .chunks_exact(layout.stride)
            .skip(y / 2)
            .take(height / 2)
        {
            output.extend_from_slice(&row[x..end_x]);
        }
        Ok(output)
    }
}
