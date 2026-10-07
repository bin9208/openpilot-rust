use serde::Serialize;
use thiserror::Error;

use crate::{exposure::CameraId, sensor::SensorKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct Geometry {
    pub width: i32,
    pub height: i32,
    pub focal_mm: f32,
}

impl Geometry {
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "source exposure rectangle uses binary32 scaling and truncation into pixel coordinates"
    )]
    pub fn exposure_region(self, sensor: SensorKind, camera: CameraId) -> Region {
        let (reference_y, reference_width, reference_height, reference_focal) = match camera {
            CameraId::Wide => (400, 1734, 524, 567.0),
            CameraId::Road => (160, 1734, 986, 2648.0),
            CameraId::Driver => (242, 1736, 906, 567.0),
        };
        let config = sensor.config();
        let focal = self.focal_mm / config.pixel_size_mm / config.out_scale as f32;
        let ratio = focal / reference_focal;
        let half_width = (ratio * reference_width as f32 / 2.0) as i32;
        let y_shift = (ratio * (604 - reference_y) as f32) as i32;
        Region {
            x: 0.max(self.width / 2 - half_width),
            y: 0.max(self.height / 2 - y_shift),
            width: ((ratio * reference_width as f32) as i32).min(self.width / 2 + half_width),
            height: ((ratio * reference_height as f32) as i32).min(self.height / 2 + y_shift),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Sampling {
    pub width: usize,
    pub x_skip: usize,
    pub y_skip: usize,
}

#[derive(Debug, Error)]
#[error("exposure sample region is outside the image or has a zero sampling interval")]
pub struct SamplingError;

pub fn luminance(pixels: &[u8], region: Region, sampling: Sampling) -> Result<f32, SamplingError> {
    let x = usize::try_from(region.x).map_err(|_| SamplingError)?;
    let y = usize::try_from(region.y).map_err(|_| SamplingError)?;
    let width = usize::try_from(region.width).map_err(|_| SamplingError)?;
    let height = usize::try_from(region.height).map_err(|_| SamplingError)?;
    if sampling.x_skip == 0 || sampling.y_skip == 0 {
        return Err(SamplingError);
    }
    let end_x = x.checked_add(width).ok_or(SamplingError)?;
    let end_y = y.checked_add(height).ok_or(SamplingError)?;
    if end_x > sampling.width {
        return Err(SamplingError);
    }
    let mut histogram = [0_u32; 256];
    let mut total = 0_u32;
    for row in (y..end_y).step_by(sampling.y_skip) {
        let start = row
            .checked_mul(sampling.width)
            .and_then(|base| base.checked_add(x))
            .ok_or(SamplingError)?;
        let end = start.checked_add(width).ok_or(SamplingError)?;
        for pixel in pixels
            .get(start..end)
            .ok_or(SamplingError)?
            .iter()
            .step_by(sampling.x_skip)
        {
            histogram[usize::from(*pixel)] = histogram[usize::from(*pixel)].wrapping_add(1);
            total = total.wrapping_add(1);
        }
    }
    let mut accumulated = 0_u32;
    for value in (0..=255_u8).rev() {
        accumulated = accumulated.wrapping_add(histogram[usize::from(value)]);
        if accumulated >= total / 2 {
            return Ok(f32::from(value) / 256.0);
        }
    }
    Ok(-1.0 / 256.0)
}
