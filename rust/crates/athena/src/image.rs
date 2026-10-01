use crate::Error;
use num_traits::ToPrimitive;
use openpilot_msgq::VisionMetadata;

pub fn extract(data: &[u8], layout: &VisionMetadata) -> Result<Vec<u8>, Error> {
    let (width, height, stride) = (layout.width, layout.height, layout.stride);
    let uv_height = ((height / 2)
        .checked_add(15)
        .ok_or(Error::Contract("image size"))?
        / 16)
        * 16;
    let uv_end = stride
        .checked_mul(uv_height)
        .and_then(|size| size.checked_add(layout.uv_offset))
        .ok_or(Error::Contract("image size"))?;
    let pixels = width
        .checked_mul(height)
        .and_then(|size| size.checked_mul(3))
        .ok_or(Error::Contract("image size"))?;
    if width % 2 != 0
        || height % 2 != 0
        || stride % 2 != 0
        || width > stride
        || uv_end > data.len()
        || stride
            .checked_mul(height)
            .is_none_or(|size| size > layout.uv_offset)
    {
        return Err(Error::Contract("invalid NV12 image layout"));
    }
    let mut rgb = Vec::with_capacity(pixels);
    for row in 0..height {
        for column in 0..width {
            let y = f64::from(data[row * stride + column]);
            let uv = layout.uv_offset + row / 2 * stride + column / 2 * 2;
            let u = f64::from(data[uv]) - 128.;
            let v = f64::from(data[uv + 1]) - 128.;
            for value in [
                y + 1.13983 * v,
                (y - 0.39465 * u) - 0.58060 * v,
                y + 2.03211 * u,
            ] {
                rgb.push(
                    value
                        .clamp(0., 255.)
                        .to_u8()
                        .ok_or(Error::Contract("nonfinite RGB value"))?,
                );
            }
        }
    }
    Ok(rgb)
}
pub fn jpeg(rgb: &[u8], width: usize, height: usize) -> Result<Vec<u8>, Error> {
    Ok(openpilot_jpeg::encode(
        rgb,
        u32::try_from(width).map_err(|_| Error::Contract("JPEG width"))?,
        u32::try_from(height).map_err(|_| Error::Contract("JPEG height"))?,
    )?)
}
