use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Serialize)]
pub struct Nv12Layout {
    pub stride: u32,
    pub y_height: u32,
    pub uv_height: u32,
    pub size: u32,
}

#[derive(Debug, Error)]
#[error("NV12 dimensions overflow the camera allocation format")]
pub struct LayoutError;

fn align(value: u32, alignment: u32) -> Result<u32, LayoutError> {
    Ok(value.checked_add(alignment - 1).ok_or(LayoutError)? & !(alignment - 1))
}

impl Nv12Layout {
    pub fn new(width: u32, height: u32) -> Result<Self, LayoutError> {
        let stride = align(width, 128)?;
        let y_height = align(height, 32)?;
        let uv_height = align(height.checked_add(1).ok_or(LayoutError)? >> 1, 16)?;
        let size = if width == 0 || height == 0 {
            0
        } else {
            let y_plane = stride.checked_mul(y_height).ok_or(LayoutError)?;
            let uv_plane = stride
                .checked_mul(uv_height)
                .and_then(|x| x.checked_add(4096))
                .ok_or(LayoutError)?;
            let extra = stride.checked_mul(8).ok_or(LayoutError)?.max(16 * 1024);
            let size = align(
                y_plane
                    .checked_add(uv_plane)
                    .and_then(|x| x.checked_add(extra))
                    .ok_or(LayoutError)?,
                4096,
            )?;
            let padding = align(width, 512)?.checked_mul(512).ok_or(LayoutError)?;
            align(size.checked_add(padding).ok_or(LayoutError)?, 4096)?
        };
        Ok(Self {
            stride,
            y_height,
            uv_height,
            size,
        })
    }
}
