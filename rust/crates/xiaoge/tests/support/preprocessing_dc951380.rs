// dc951380 lane_image lines 13-25 below are copied byte-for-byte.
// Imports and the geometry module declaration are test harness adapters only.
// See preprocessing_dc951380.json for source and extraction SHA-256 receipts.
use openpilot_opencv_runtime::{self as cv, Dimensions, Format, Image, ImageLayout};
use openpilot_xiaoge::{inference::Error, nv12::Frame};

#[path = "geometry_dc951380.rs"]
pub mod geometry;

pub fn lane_image(frame: &Frame<'_>) -> Result<Image, Error> {
    let layout = frame.layout();
    let size = u32::try_from(layout.width.min(layout.height))
        .map_err(|_| Error::Contract("lane image dimension overflow"))?;
    let image = Image::from_parts(
        ImageLayout::new(Dimensions::new(size, size)?, Format::Gray)?,
        frame.center_square()?,
    )?;
    if size == 416 {
        return Ok(image);
    }
    Ok(cv::resize_linear(image.view(), Dimensions::new(416, 416)?)?)
}
