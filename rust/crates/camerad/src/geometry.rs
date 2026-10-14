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
    Ok(histogram_luminance(&histogram, total))
}

/// Same source upper-histogram rule for an owned compact set of sampled bytes.
pub fn luminance_samples(samples: &[u8]) -> f32 {
    let mut histogram = [0_u32; 256];
    let mut total = 0_u32;
    for pixel in samples {
        histogram[usize::from(*pixel)] = histogram[usize::from(*pixel)].wrapping_add(1);
        total = total.wrapping_add(1);
    }
    histogram_luminance(&histogram, total)
}

fn histogram_luminance(histogram: &[u32; 256], total: u32) -> f32 {
    let mut accumulated = 0_u32;
    for value in (0..=255_u8).rev() {
        accumulated = accumulated.wrapping_add(histogram[usize::from(value)]);
        if accumulated >= total / 2 {
            return f32::from(value) / 256.0;
        }
    }
    -1.0 / 256.0
}

/// Exact source AE byte offsets: row * out_img_width + x, including padded NV12.
/// This intentionally does not reinterpret the source algorithm using NV12 stride.
#[derive(Clone, Debug)]
pub struct SampleIndices {
    start: usize,
    row_step: usize,
    x_step: usize,
    columns: usize,
    count: usize,
    next: usize,
}
impl Iterator for SampleIndices {
    type Item = usize;
    fn next(&mut self) -> Option<usize> {
        if self.next == self.count {
            return None;
        }
        let offset = self.start
            + self.next / self.columns * self.row_step
            + self.next % self.columns * self.x_step;
        self.next += 1;
        Some(offset)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.count - self.next;
        (remaining, Some(remaining))
    }
    fn count(self) -> usize {
        self.count - self.next
    }
}
impl ExactSizeIterator for SampleIndices {}

pub fn sample_indices(
    region: Region,
    sampling: Sampling,
    pixel_length: usize,
) -> Result<SampleIndices, SamplingError> {
    let x = usize::try_from(region.x).map_err(|_| SamplingError)?;
    let y = usize::try_from(region.y).map_err(|_| SamplingError)?;
    let width = usize::try_from(region.width).map_err(|_| SamplingError)?;
    let height = usize::try_from(region.height).map_err(|_| SamplingError)?;
    if sampling.x_skip == 0 || sampling.y_skip == 0 {
        return Err(SamplingError);
    }
    let end_x = x.checked_add(width).ok_or(SamplingError)?;
    y.checked_add(height).ok_or(SamplingError)?;
    if end_x > sampling.width {
        return Err(SamplingError);
    }
    let columns = if width == 0 {
        0
    } else {
        (width - 1) / sampling.x_skip + 1
    };
    let rows = if height == 0 {
        0
    } else {
        (height - 1) / sampling.y_skip + 1
    };
    if rows != 0 {
        let last_row = (rows - 1)
            .checked_mul(sampling.y_skip)
            .and_then(|offset| y.checked_add(offset))
            .ok_or(SamplingError)?;
        let end = last_row
            .checked_mul(sampling.width)
            .and_then(|offset| offset.checked_add(end_x))
            .ok_or(SamplingError)?;
        if end > pixel_length {
            return Err(SamplingError);
        }
    }
    let count = columns.checked_mul(rows).ok_or(SamplingError)?;
    let start = if count == 0 {
        0
    } else {
        y.checked_mul(sampling.width)
            .and_then(|offset| offset.checked_add(x))
            .ok_or(SamplingError)?
    };
    let row_step = if rows > 1 && columns > 0 {
        sampling
            .width
            .checked_mul(sampling.y_skip)
            .ok_or(SamplingError)?
    } else {
        0
    };
    Ok(SampleIndices {
        start,
        row_step,
        x_step: sampling.x_skip,
        columns,
        count,
        next: 0,
    })
}
