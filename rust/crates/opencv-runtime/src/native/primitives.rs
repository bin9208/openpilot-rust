use super::{native_error, ready, size};
use crate::{bridge::ffi, Dimensions, Error, Format, Image, ImageLayout, ImageView, Rect};
#[derive(Debug)]
pub struct Component {
    pub bounds: [i32; 4],
    pub area: i32,
    pub center: [f64; 2],
}
#[derive(Debug)]
pub struct Components {
    pub labels: Vec<i32>,
    pub stats: Vec<Component>,
}
#[derive(Debug)]
pub struct Contour {
    pub bounds: Rect,
    pub area: f64,
}
fn format(input: ImageView<'_>, expected: Format) -> Result<(), Error> {
    if input.layout().format() != expected {
        return Err(Error::Contract("OpenCV primitive input format mismatch"));
    }
    ready()
}
pub fn rgb_to_gray(input: ImageView<'_>) -> Result<Image, Error> {
    format(input, Format::Rgb)?;
    let dims = input.layout().dimensions();
    let data = ffi::rgb_gray(input.data(), size(dims)).map_err(|e| native_error("RGB2GRAY", e))?;
    Image::from_parts(ImageLayout::new(dims, Format::Gray)?, data)
}
pub fn rgb_to_hsv(input: ImageView<'_>) -> Result<Image, Error> {
    format(input, Format::Rgb)?;
    let dims = input.layout().dimensions();
    let data = ffi::rgb_hsv(input.data(), size(dims)).map_err(|e| native_error("RGB2HSV", e))?;
    Image::from_parts(ImageLayout::new(dims, Format::Hsv)?, data)
}
pub fn connected_components(input: ImageView<'_>) -> Result<Components, Error> {
    format(input, Format::Gray)?;
    let out = ffi::components(input.data(), size(input.layout().dimensions()))
        .map_err(|e| native_error("connectedComponentsWithStats", e))?;
    Ok(Components {
        labels: out.labels,
        stats: out
            .stats
            .into_iter()
            .map(|s| Component {
                bounds: [s.x, s.y, s.width, s.height],
                area: s.area,
                center: [s.center_x, s.center_y],
            })
            .collect(),
    })
}
pub fn morphology_open(input: ImageView<'_>, kernel: Dimensions) -> Result<Image, Error> {
    format(input, Format::Gray)?;
    let dims = input.layout().dimensions();
    let data = ffi::morphology_open(input.data(), size(dims), size(kernel))
        .map_err(|e| native_error("morphologyEx OPEN", e))?;
    Image::from_parts(ImageLayout::new(dims, Format::Gray)?, data)
}
pub fn external_contours(input: ImageView<'_>) -> Result<Vec<Contour>, Error> {
    format(input, Format::Gray)?;
    let out = ffi::external_contours(input.data(), size(input.layout().dimensions()))
        .map_err(|e| native_error("findContours EXTERNAL SIMPLE", e))?;
    out.into_iter()
        .map(|c| {
            Ok(Contour {
                bounds: Rect {
                    x: c.bounds.x,
                    y: c.bounds.y,
                    width: c.bounds.width,
                    height: c.bounds.height,
                },
                area: c.area,
            })
        })
        .collect()
}
pub fn match_template(
    input: ImageView<'_>,
    template: ImageView<'_>,
    mask: Option<ImageView<'_>>,
) -> Result<Vec<f32>, Error> {
    format(input, Format::Gray)?;
    format(template, Format::Gray)?;
    let (dims, patch) = (input.layout().dimensions(), template.layout().dimensions());
    if patch.width() > dims.width() || patch.height() > dims.height() {
        return Err(Error::Contract("template exceeds source"));
    }
    let mask = if let Some(mask) = mask {
        format(mask, Format::Gray)?;
        if mask.layout().dimensions() != patch {
            return Err(Error::Contract("template mask differs from template"));
        }
        mask.data()
    } else {
        &[]
    };
    ffi::match_template(input.data(), size(dims), template.data(), size(patch), mask)
        .map_err(|e| native_error("matchTemplate CCOEFF_NORMED", e))
}

pub fn match_template_ccorr(
    input: ImageView<'_>,
    template: ImageView<'_>,
    mask: Option<ImageView<'_>>,
) -> Result<Vec<f32>, Error> {
    format(input, Format::Gray)?;
    format(template, Format::Gray)?;
    let (dims, patch) = (input.layout().dimensions(), template.layout().dimensions());
    if patch.width() > dims.width() || patch.height() > dims.height() {
        return Err(Error::Contract("template exceeds source"));
    }
    let mask = if let Some(mask) = mask {
        format(mask, Format::Gray)?;
        if mask.layout().dimensions() != patch {
            return Err(Error::Contract("template mask differs from template"));
        }
        mask.data()
    } else {
        &[]
    };
    ffi::match_template_ccorr(input.data(), size(dims), template.data(), size(patch), mask)
        .map_err(|e| native_error("matchTemplate CCORR_NORMED", e))
}

pub fn morphology_tophat(input: ImageView<'_>, kernel: Dimensions) -> Result<Image, Error> {
    format(input, Format::Gray)?;
    let dims = input.layout().dimensions();
    let data = ffi::morphology_tophat(input.data(), size(dims), size(kernel))
        .map_err(|e| native_error("morphologyEx TOPHAT", e))?;
    Image::from_parts(ImageLayout::new(dims, Format::Gray)?, data)
}
pub fn distance_transform(input: ImageView<'_>) -> Result<Vec<f32>, Error> {
    format(input, Format::Gray)?;
    ffi::distance_transform(input.data(), size(input.layout().dimensions()))
        .map_err(|e| native_error("distanceTransform L2 mask5", e))
}
