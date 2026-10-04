use super::{layout, native_error, ready, size};
use crate::bridge::ffi;
use crate::{Dimensions, Error, Format, Image, ImageLayout, ImageView, Point, Rect};

pub fn resize_linear(input: ImageView<'_>, output: Dimensions) -> Result<Image, Error> {
    ready()?;
    let data = ffi::resize(input.data(), layout(input.layout()), size(output))
        .map_err(|error| native_error("resize", error))?;
    Image::from_parts(ImageLayout::new(output, input.layout().format())?, data)
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
