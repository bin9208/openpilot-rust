use crate::Error;
#[cfg(feature = "native")]
use ffmpeg_next::{
    format::Pixel,
    frame::Video,
    software::scaling::{Context, Flags},
};

fn extent(bytes: &[u8], width: u32, height: u32) -> Result<(usize, usize), Error> {
    i32::try_from(width)?;
    i32::try_from(height)?;
    let (width, height) = (usize::try_from(width)?, usize::try_from(height)?);
    let length = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(3));
    if width == 0 || height == 0 || length != Some(bytes.len()) {
        return Err(Error::Contract("BGR24 dimensions differ from packed bytes"));
    }
    Ok((width, height))
}

/// Rotate packed BGR pixels by 180 degrees without changing channel order.
///
/// # Errors
/// Rejects empty, overflowing or mismatched image extents.
pub fn rotate(bytes: &mut [u8], width: u32, height: u32) -> Result<(), Error> {
    extent(bytes, width, height)?;
    bytes.reverse();
    for pixel in bytes.chunks_exact_mut(3) {
        pixel.reverse();
    }
    Ok(())
}

#[cfg(feature = "native")]
fn append_rows(
    output: &mut Vec<u8>,
    frame: &Video,
    plane: usize,
    width: usize,
    rows: usize,
) -> Result<(), Error> {
    let stride = frame.stride(plane);
    if stride < width {
        return Err(Error::Contract("scaler plane stride is shorter than a row"));
    }
    for row in frame.data(plane).chunks_exact(stride).take(rows) {
        output.extend_from_slice(&row[..width]);
    }
    Ok(())
}

/// Reproduce `PyAV`'s default BGR24 to packed NV12 reformat operation.
///
/// # Errors
/// Rejects invalid extents and returns external libswscale errors.
#[cfg(feature = "native")]
pub fn nv12(bytes: &[u8], width: u32, height: u32) -> Result<Vec<u8>, Error> {
    let (w, h) = extent(bytes, width, height)?;
    let uv_width = w
        .div_ceil(2)
        .checked_mul(2)
        .ok_or(Error::Contract("UV width overflow"))?;
    let uv_height = h.div_ceil(2);
    let length = w
        .checked_mul(h)
        .and_then(|y| {
            uv_width
                .checked_mul(uv_height)
                .and_then(|uv| y.checked_add(uv))
        })
        .ok_or(Error::Contract("NV12 length overflow"))?;
    if !length.is_multiple_of(w) {
        return Err(Error::Contract(
            "NV12 planes cannot reshape to the source ndarray width",
        ));
    }
    let mut input = Video::new(Pixel::BGR24, width, height);
    let stride = input.stride(0);
    let row = w
        .checked_mul(3)
        .ok_or(Error::Contract("BGR row overflow"))?;
    if stride < row {
        return Err(Error::Contract("BGR frame stride is shorter than a row"));
    }
    for (target, source) in input
        .data_mut(0)
        .chunks_mut(stride)
        .take(h)
        .zip(bytes.chunks_exact(row))
    {
        target.fill(0);
        target[..row].copy_from_slice(source);
    }
    // The source's libswscale 9 disables half-chroma input for odd RGB widths.
    // The public flag selects that same provider path on libswscale 7.
    let flags = if width.is_multiple_of(2) {
        Flags::BILINEAR
    } else {
        Flags::BILINEAR | Flags::FULL_CHR_H_INP
    };
    let mut scaler = Context::get(
        Pixel::BGR24,
        width,
        height,
        Pixel::NV12,
        width,
        height,
        flags,
    )?;
    let mut frame = Video::empty();
    scaler.run(&input, &mut frame)?;
    let mut output = Vec::with_capacity(length);
    append_rows(&mut output, &frame, 0, w, h)?;
    append_rows(&mut output, &frame, 1, uv_width, uv_height)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::rotate;

    #[test]
    fn rotation_moves_whole_bgr_pixels_across_both_axes() {
        let mut image = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        rotate(&mut image, 2, 2).unwrap();
        assert_eq!(image, [10, 11, 12, 7, 8, 9, 4, 5, 6, 1, 2, 3]);
    }

    #[test]
    fn rotation_rejects_a_short_bgr_frame_without_mutating_bytes() {
        let mut image = vec![1, 2, 3];
        assert!(rotate(&mut image, 2, 2).is_err());
        assert_eq!(image, [1, 2, 3]);
    }
}
