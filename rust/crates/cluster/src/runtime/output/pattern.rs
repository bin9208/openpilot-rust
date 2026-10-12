use super::super::{Error, Result};
use crate::renderer::nv12::Nv12Layout;

/// # Errors
/// Rejects dimensions or allocation lengths outside the native representation.
pub fn rgba(width: i32, height: i32) -> Result<Vec<u8>> {
    let width = usize::try_from(width).map_err(|error| Error::Invalid(error.to_string()))?;
    let height = usize::try_from(height).map_err(|error| Error::Invalid(error.to_string()))?;
    let half_width = (width / 2).max(1);
    let half_height = (height / 2).max(1);
    let length = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| Error::Invalid("RGBA pattern allocation overflow".into()))?;
    let mut bytes = Vec::with_capacity(length);
    for y in 0..height {
        for x in 0..width {
            bytes.extend_from_slice(match (y < half_height, x < half_width) {
                (true, true) => &[255, 0, 0, 255],
                (true, false) => &[0, 255, 0, 255],
                (false, true) => &[0, 0, 255, 255],
                (false, false) => &[255, 255, 255, 255],
            });
        }
    }
    Ok(bytes)
}

/// # Errors
/// Rejects layouts that cannot contain the source pattern and padding.
pub fn nv12(layout: Nv12Layout) -> Result<Vec<u8>> {
    layout.validate()?;
    let stride =
        usize::try_from(layout.stride).map_err(|error| Error::Invalid(error.to_string()))?;
    let uv_rows =
        usize::try_from(layout.uv_scanlines).map_err(|error| Error::Invalid(error.to_string()))?;
    let uv_end = layout.uv_offset + stride * uv_rows;
    let mut bytes = vec![16; layout.byte_count];
    bytes[layout.uv_offset..uv_end].fill(128);
    let width = usize::try_from(layout.width).map_err(|error| Error::Invalid(error.to_string()))?;
    let height =
        usize::try_from(layout.height).map_err(|error| Error::Invalid(error.to_string()))?;
    let middle_x = (width / 2).max(2);
    let middle_y = (height / 2).max(2);
    for (x0, y0, x1, y1, y_value, u, v) in [
        (0, 0, middle_x, middle_y, 82, 90, 240),
        (middle_x, 0, width, middle_y, 145, 54, 34),
        (0, middle_y, middle_x, height, 41, 240, 110),
        (middle_x, middle_y, width, height, 235, 128, 128),
    ] {
        for y in y0..y1 {
            bytes[y * stride + x0..y * stride + x1].fill(y_value);
        }
        for y in (y0 & !1..y1).step_by(2) {
            for x in (x0 & !1..x1).step_by(2) {
                let offset = layout.uv_offset + (y / 2) * stride + x;
                if offset + 1 < uv_end {
                    bytes[offset] = u;
                    bytes[offset + 1] = v;
                }
            }
        }
    }
    Ok(bytes)
}
