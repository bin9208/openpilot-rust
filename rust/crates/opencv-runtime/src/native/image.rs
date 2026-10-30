use super::{layout, native_error, ready, size};
use crate::bridge::ffi;
use crate::{Dimensions, Error, Format, Image, ImageLayout, ImageView, Point, Rect};

pub fn resize_linear(input: ImageView<'_>, output: Dimensions) -> Result<Image, Error> {
    ready()?;
    let data = ffi::resize(input.data(), layout(input.layout()), size(output))
        .map_err(|error| native_error("resize", error))?;
    Image::from_parts(ImageLayout::new(output, input.layout().format())?, data)
}

/// Resize a grayscale ROI without packing rows. The complete source Y allocation
/// (including final-row padding) must be present; odd strides are supported.
pub fn resize_gray_roi(
    input: &[u8],
    dimensions: Dimensions,
    stride: usize,
    roi: Rect,
    output: Dimensions,
) -> Result<Image, Error> {
    let extent = stride
        .checked_mul(dimensions.height() as usize)
        .ok_or(Error::Contract("grayscale plane extent overflow"))?;
    if stride < dimensions.width() as usize
        || input.len() < extent
        || roi.x < 0
        || roi.y < 0
        || roi.width == 0
        || roi.height == 0
        || i64::from(roi.x) + i64::from(roi.width) > i64::from(dimensions.width())
        || i64::from(roi.y) + i64::from(roi.height) > i64::from(dimensions.height())
    {
        return Err(Error::Contract(
            "strided grayscale ROI differs from checked plane extent",
        ));
    }
    ready()?;
    let data = ffi::resize_gray_roi(
        input,
        size(dimensions),
        stride,
        ffi::Rect {
            x: roi.x,
            y: roi.y,
            width: roi.width,
            height: roi.height,
        },
        size(output),
    )
    .map_err(|error| native_error("resize grayscale ROI", error))?;
    Image::from_parts(ImageLayout::new(output, Format::Gray)?, data)
}

pub fn bgr_to_gray(input: ImageView<'_>) -> Result<Image, Error> {
    if input.layout().format() != Format::Bgr {
        return Err(Error::Contract("BGR grayscale conversion needs BGR pixels"));
    }
    ready()?;
    let dimensions = input.layout().dimensions();
    let data = ffi::bgr_gray(input.data(), size(dimensions))
        .map_err(|error| native_error("cvtColor BGR2GRAY", error))?;
    Image::from_parts(ImageLayout::new(dimensions, Format::Gray)?, data)
}

pub fn nv12_to_rgb(input: &[u8], dimensions: Dimensions) -> Result<Image, Error> {
    if input.len() != dimensions.nv12_len()? {
        return Err(Error::Contract(
            "NV12 slice differs from packed plane length",
        ));
    }
    ready()?;
    let data = ffi::nv12_rgb(input, size(dimensions))
        .map_err(|error| native_error("cvtColor NV12", error))?;
    Image::from_parts(ImageLayout::new(dimensions, Format::Rgb)?, data)
}

/// Keep the conversion and masked image native, copying only resized RGB back.
pub fn nv12_masked_resize_rgb(
    input: &[u8],
    dimensions: Dimensions,
    mask: ImageView<'_>,
    output: Dimensions,
) -> Result<Image, Error> {
    if input.len() != dimensions.nv12_len()? {
        return Err(Error::Contract(
            "NV12 slice differs from packed plane length",
        ));
    }
    if mask.layout().format() != Format::Gray || mask.layout().dimensions() != dimensions {
        return Err(Error::Contract(
            "mask must be grayscale with matching dimensions",
        ));
    }
    ready()?;
    let data = ffi::nv12_masked_resize_rgb(input, size(dimensions), mask.data(), size(output))
        .map_err(|error| native_error("NV12 conversion, mask and resize", error))?;
    Image::from_parts(ImageLayout::new(output, Format::Rgb)?, data)
}

/// Convert padded borrowed planes directly; the result owns all RGB storage.
pub fn nv12_planes_to_rgb(
    input: &[u8],
    dimensions: Dimensions,
    stride: usize,
    uv_offset: usize,
) -> Result<Image, Error> {
    dimensions.nv12_len()?;
    let y_end = stride
        .checked_mul(dimensions.height() as usize)
        .ok_or(Error::Contract("NV12 plane extent overflow"))?;
    let end = stride
        .checked_mul(dimensions.height() as usize / 2)
        .and_then(|n| uv_offset.checked_add(n))
        .ok_or(Error::Contract("NV12 plane extent overflow"))?;
    if stride < dimensions.width() as usize
        || !stride.is_multiple_of(2)
        || uv_offset < y_end
        || input.len() < end
    {
        return Err(Error::Contract(
            "NV12 padded planes differ from checked extent",
        ));
    }
    ready()?;
    let data = ffi::nv12_planes_rgb(input, size(dimensions), stride, uv_offset)
        .map_err(|error| native_error("cvtColorTwoPlane NV12", error))?;
    Image::from_parts(ImageLayout::new(dimensions, Format::Rgb)?, data)
}

fn points(input: &[Point]) -> Result<Vec<ffi::Point>, Error> {
    crate::image::validate_points(input)?;
    Ok(input
        .iter()
        .map(|point| ffi::Point {
            x: point.x,
            y: point.y,
        })
        .collect())
}

pub fn polygon_mask(dimensions: Dimensions, polygon: &[Point]) -> Result<Image, Error> {
    if polygon.len() < 3 {
        return Err(Error::Contract("mask polygon needs three points"));
    }
    let points = points(polygon)?;
    ready()?;
    let data = ffi::mask_polygon(&points, size(dimensions))
        .map_err(|error| native_error("fillPoly", error))?;
    Image::from_parts(ImageLayout::new(dimensions, Format::Gray)?, data)
}

pub fn bounding_rect(polygon: &[Point]) -> Result<Rect, Error> {
    let points = points(polygon)?;
    ready()?;
    let result = ffi::bounds(&points).map_err(|error| native_error("boundingRect", error))?;
    Ok(Rect {
        x: result.x,
        y: result.y,
        width: result.width,
        height: result.height,
    })
}

pub fn apply_mask(input: ImageView<'_>, mask: ImageView<'_>) -> Result<Image, Error> {
    if mask.layout().format() != Format::Gray
        || mask.layout().dimensions() != input.layout().dimensions()
    {
        return Err(Error::Contract(
            "mask must be grayscale with matching dimensions",
        ));
    }
    ready()?;
    let data = ffi::mask_image(input.data(), mask.data(), layout(input.layout()))
        .map_err(|error| native_error("bitwise_and", error))?;
    Image::from_parts(input.layout(), data)
}
