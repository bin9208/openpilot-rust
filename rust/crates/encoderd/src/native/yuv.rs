#![allow(unsafe_code)]
use super::Mapping;
use crate::Error;
use openpilot_msgq::VisionMetadata;

unsafe extern "C" {
    fn NV12ToI420(
        y: *const u8,
        stride: i32,
        uv: *const u8,
        uv_stride: i32,
        out_y: *mut u8,
        out_stride: i32,
        out_u: *mut u8,
        u_stride: i32,
        out_v: *mut u8,
        v_stride: i32,
        width: i32,
        height: i32,
    ) -> i32;
    fn I420Scale(
        y: *const u8,
        stride: i32,
        u: *const u8,
        u_stride: i32,
        v: *const u8,
        v_stride: i32,
        width: i32,
        height: i32,
        out_y: *mut u8,
        out_stride: i32,
        out_u: *mut u8,
        out_u_stride: i32,
        out_v: *mut u8,
        out_v_stride: i32,
        out_width: i32,
        out_height: i32,
        filter: i32,
    ) -> i32;
}

pub fn size(width: i32, height: i32) -> Result<usize, Error> {
    if width <= 0 || height <= 0 || width % 2 != 0 || height % 2 != 0 {
        return Err(Error::Contract("I420 dimensions must be positive and even"));
    }
    let pixels = usize::try_from(width)?
        .checked_mul(usize::try_from(height)?)
        .ok_or(Error::Contract("I420 size overflow"))?;
    pixels
        .checked_mul(3)
        .map(|size| size / 2)
        .ok_or(Error::Contract("I420 size overflow"))
}

pub fn convert(
    mapping: &Mapping,
    metadata: &VisionMetadata,
    output: &mut [u8],
) -> Result<(), Error> {
    let width = i32::try_from(metadata.width)?;
    let height = i32::try_from(metadata.height)?;
    let stride = i32::try_from(metadata.stride)?;
    let y_end = metadata
        .stride
        .checked_mul(metadata.height)
        .ok_or(Error::Contract("NV12 luma size overflow"))?;
    let uv_end = metadata
        .stride
        .checked_mul(metadata.height / 2)
        .and_then(|len| metadata.uv_offset.checked_add(len))
        .ok_or(Error::Contract("NV12 chroma size overflow"))?;
    if metadata.stride < metadata.width
        || metadata.uv_offset < y_end
        || uv_end > metadata.len
        || metadata.len > mapping.length()
    {
        return Err(Error::Contract("NV12 input exceeds validated mapping"));
    }
    if output.len() != size(width, height)? {
        return Err(Error::Contract("I420 conversion buffer size"));
    }
    let pixels = metadata.width * metadata.height;
    // SAFETY: imported NV12 layout was validated; destination planes are
    // disjoint regions of the checked I420 allocation and all owners survive.
    unsafe {
        let y = mapping.address() as *const u8;
        let cy = output.as_mut_ptr();
        NV12ToI420(
            y,
            stride,
            y.add(metadata.uv_offset),
            stride,
            cy,
            width,
            cy.add(pixels),
            width / 2,
            cy.add(pixels + pixels / 4),
            width / 2,
            width,
            height,
        );
    }
    Ok(())
}

pub fn scale(
    input: &[u8],
    dimensions: (i32, i32),
    output: &mut [u8],
    target: (i32, i32),
) -> Result<(), Error> {
    if input.len() != size(dimensions.0, dimensions.1)? || output.len() != size(target.0, target.1)?
    {
        return Err(Error::Contract("I420 scaling buffer size"));
    }
    let pixels = input.len() * 2 / 3;
    let scaled_pixels = output.len() * 2 / 3;
    // SAFETY: checked even-dimension planes occupy each complete allocation;
    // libyuv consumes them synchronously using the source's kFilterNone=0.
    unsafe {
        let y = input.as_ptr();
        let cy = output.as_mut_ptr();
        I420Scale(
            y,
            dimensions.0,
            y.add(pixels),
            dimensions.0 / 2,
            y.add(pixels + pixels / 4),
            dimensions.0 / 2,
            dimensions.0,
            dimensions.1,
            cy,
            target.0,
            cy.add(scaled_pixels),
            target.0 / 2,
            cy.add(scaled_pixels + scaled_pixels / 4),
            target.0 / 2,
            target.0,
            target.1,
            0,
        );
    }
    Ok(())
}
