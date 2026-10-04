use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dimensions {
    width: u32,
    height: u32,
}

impl Dimensions {
    pub fn new(width: u32, height: u32) -> Result<Self, Error> {
        if width == 0
            || height == 0
            || width > i32::MAX.unsigned_abs()
            || height > i32::MAX.unsigned_abs()
        {
            return Err(Error::Contract(
                "dimensions must be positive OpenCV integers",
            ));
        }
        let result = Self { width, height };
        result.pixels()?;
        Ok(result)
    }
    pub const fn width(self) -> u32 {
        self.width
    }
    pub const fn height(self) -> u32 {
        self.height
    }
    pub fn pixels(self) -> Result<usize, Error> {
        usize::try_from(u64::from(self.width) * u64::from(self.height))
            .map_err(|_| Error::Contract("image allocation exceeds addressable bytes"))
    }
    pub fn nv12_len(self) -> Result<usize, Error> {
        if !self.width.is_multiple_of(2) || !self.height.is_multiple_of(2) {
            return Err(Error::Contract("packed NV12 dimensions must be even"));
        }
        self.pixels()?
            .checked_add(self.pixels()? / 2)
            .ok_or(Error::Contract("NV12 allocation exceeds addressable bytes"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Gray,
    Rgb,
    Bgr,
}

impl Format {
    pub const fn channels(self) -> usize {
        match self {
            Self::Gray => 1,
            Self::Rgb | Self::Bgr => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageLayout {
    dimensions: Dimensions,
    format: Format,
    len: usize,
}

impl ImageLayout {
    pub fn new(dimensions: Dimensions, format: Format) -> Result<Self, Error> {
        let len = dimensions
            .pixels()?
            .checked_mul(format.channels())
            .ok_or(Error::Contract(
                "image allocation exceeds addressable bytes",
            ))?;
        Ok(Self {
            dimensions,
            format,
            len,
        })
    }
    pub const fn dimensions(self) -> Dimensions {
        self.dimensions
    }
    pub const fn format(self) -> Format {
        self.format
    }
    pub const fn len(self) -> usize {
        self.len
    }
    pub const fn is_empty(self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ImageView<'a> {
    data: &'a [u8],
    layout: ImageLayout,
}

impl<'a> ImageView<'a> {
    pub fn new(data: &'a [u8], layout: ImageLayout) -> Result<Self, Error> {
        if data.len() != layout.len() {
            return Err(Error::Contract("image slice differs from packed layout"));
        }
        Ok(Self { data, layout })
    }
    pub const fn data(self) -> &'a [u8] {
        self.data
    }
    pub const fn layout(self) -> ImageLayout {
        self.layout
    }
}

#[derive(Debug, Clone)]
pub struct Image {
    data: Vec<u8>,
    layout: ImageLayout,
}

impl Image {
    pub fn from_parts(layout: ImageLayout, data: Vec<u8>) -> Result<Self, Error> {
        ImageView::new(&data, layout)?;
        Ok(Self { data, layout })
    }
    pub fn view(&self) -> ImageView<'_> {
        ImageView {
            data: &self.data,
            layout: self.layout,
        }
    }
    pub fn into_data(self) -> Vec<u8> {
        self.data
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[cfg(any(feature = "native-skip-miri", test))]
pub(crate) fn validate_points(input: &[Point]) -> Result<(), Error> {
    let Some(first) = input.first() else {
        return Err(Error::Contract("polygon needs at least one point"));
    };
    i32::try_from(input.len())
        .map_err(|_| Error::Contract("polygon point count exceeds OpenCV integer"))?;
    let (mut left, mut right, mut top, mut bottom) = (first.x, first.x, first.y, first.y);
    for point in input {
        left = left.min(point.x);
        right = right.max(point.x);
        top = top.min(point.y);
        bottom = bottom.max(point.y);
    }
    if i64::from(right) - i64::from(left) + 1 > i64::from(i32::MAX)
        || i64::from(bottom) - i64::from(top) + 1 > i64::from(i32::MAX)
    {
        return Err(Error::Contract(
            "polygon span exceeds OpenCV rectangle integers",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate_points, Point};
    #[test]
    fn polygon_span_arithmetic_is_checked_in_i64_before_native_rectangle_calls() {
        assert!(validate_points(&[]).is_err());
        assert!(
            validate_points(&[Point { x: i32::MIN, y: 0 }, Point { x: i32::MAX, y: 0 }]).is_err()
        );
        assert!(validate_points(&[Point { x: 0, y: i32::MIN }, Point { x: 0, y: 0 }]).is_err());
        assert!(validate_points(&[Point { x: -10, y: -2 }, Point { x: 15, y: 3 }]).is_ok());
    }
}
