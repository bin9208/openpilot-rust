use super::Error;
use crate::{config::Config, nv12::Frame, vision::Side};
use num_traits::ToPrimitive;
use openpilot_opencv_runtime::{self as cv, Dimensions, Image, Point, Tensor};

pub struct Region {
    bounds: [usize; 4],
    mask: Image,
}

impl Region {
    pub const fn bounds(&self) -> [usize; 4] {
        self.bounds
    }
    pub fn mask(&self) -> &Image {
        &self.mask
    }
}

pub struct Geometry {
    dimensions: Dimensions,
    regions: [Option<Region>; 2],
}

impl Geometry {
    pub fn new(config: &Config, dimensions: Dimensions) -> Result<Self, Error> {
        if config.width == 0 || config.height == 0 {
            return Err(Error::Contract("zero configuration dimension"));
        }
        dimensions.nv12_len()?;
        let mut regions = [None, None];
        for (index, raw) in [&config.poly_left, &config.poly_right]
            .into_iter()
            .enumerate()
        {
            if raw.len() < 3 {
                continue;
            }
            let scale_x = (f64::from(dimensions.width()) / f64::from(config.width))
                .to_f32()
                .ok_or(Error::Contract("polygon x scale overflow"))?;
            let scale_y = (f64::from(dimensions.height()) / f64::from(config.height))
                .to_f32()
                .ok_or(Error::Contract("polygon y scale overflow"))?;
            let points = raw
                .iter()
                .map(|&[x, y]| {
                    let x = (x.to_f32().ok_or(Error::Contract("polygon x overflow"))? * scale_x)
                        .to_i32()
                        .ok_or(Error::Contract("scaled polygon x overflow"))?;
                    let y = (y.to_f32().ok_or(Error::Contract("polygon y overflow"))? * scale_y)
                        .to_i32()
                        .ok_or(Error::Contract("scaled polygon y overflow"))?;
                    Ok(Point { x, y })
                })
                .collect::<Result<Vec<_>, Error>>()?;
            let original = cv::bounding_rect(&points)?;
            let width = i32::try_from(dimensions.width())
                .map_err(|_| Error::Contract("frame width overflow"))?;
            let height = i32::try_from(dimensions.height())
                .map_err(|_| Error::Contract("frame height overflow"))?;
            let x = (original.x.div_euclid(2) * 2).min(width - 2).max(0);
            let y = (original.y.div_euclid(2) * 2).min(height - 2).max(0);
            let available_width =
                u32::try_from(width - x).map_err(|_| Error::Contract("crop width overflow"))?;
            let available_height =
                u32::try_from(height - y).map_err(|_| Error::Contract("crop height overflow"))?;
            let crop_width = (original.width.div_ceil(2) * 2).min(available_width).max(2) / 2 * 2;
            let crop_height = (original.height.div_ceil(2) * 2)
                .min(available_height)
                .max(2)
                / 2
                * 2;
            let relative: Vec<_> = points
                .iter()
                .map(|point| Point {
                    x: point.x - x,
                    y: point.y - y,
                })
                .collect();
            let mask = cv::polygon_mask(Dimensions::new(crop_width, crop_height)?, &relative)?;
            let values = [
                u32::try_from(x).map_err(|_| Error::Contract("crop x overflow"))?,
                u32::try_from(y).map_err(|_| Error::Contract("crop y overflow"))?,
                crop_width,
                crop_height,
            ];
            let mut bounds = [0; 4];
            for (slot, value) in bounds.iter_mut().zip(values) {
                *slot = usize::try_from(value)
                    .map_err(|_| Error::Contract("crop exceeds address space"))?;
            }
            regions[index] = Some(Region { bounds, mask });
        }
        Ok(Self {
            dimensions,
            regions,
        })
    }

    pub const fn dimensions(&self) -> Dimensions {
        self.dimensions
    }
    pub fn region(&self, side: Side) -> Option<&Region> {
        self.regions[side.index()].as_ref()
    }

    pub fn tensor(&self, frame: &Frame<'_>, side: Side) -> Result<Option<Tensor>, Error> {
        let Some(region) = self.region(side) else {
            return Ok(None);
        };
        let layout = frame.layout();
        let dimensions = Dimensions::new(
            u32::try_from(layout.width).map_err(|_| Error::Contract("frame width overflow"))?,
            u32::try_from(layout.height).map_err(|_| Error::Contract("frame height overflow"))?,
        )?;
        if dimensions != self.dimensions {
            return Err(Error::Contract(
                "blindspot geometry differs from frame dimensions",
            ));
        }
        let packed = frame.pack()?;
        let [x, y, width, height] = region.bounds;
        let mut cropped = Vec::with_capacity(width * (height + height / 2));
        for row in y..y + height {
            let offset = row * layout.width + x;
            cropped.extend_from_slice(&packed[offset..offset + width]);
        }
        for row in layout.height + y / 2..layout.height + (y + height) / 2 {
            let offset = row * layout.width + x;
            cropped.extend_from_slice(&packed[offset..offset + width]);
        }
        let rgb = cv::nv12_to_rgb(&cropped, region.mask.view().layout().dimensions())?;
        let masked = cv::apply_mask(rgb.view(), region.mask.view())?;
        let resized = cv::resize_linear(masked.view(), Dimensions::new(352, 256)?)?;
        let mut values = Vec::with_capacity(3 * 352 * 256);
        for channel in 0..3 {
            values.extend(
                resized
                    .view()
                    .data()
                    .chunks_exact(3)
                    .map(|pixel| f32::from(pixel[channel]) / 255.0),
            );
        }
        Ok(Some(Tensor::from_parts(vec![1, 3, 256, 352], values)?))
    }
}
