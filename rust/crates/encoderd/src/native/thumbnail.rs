#![allow(unsafe_code)]
use super::Mapping;
use crate::Error;
use openpilot_msgq::VisionMetadata;

#[allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    dead_code,
    clippy::all
)]
mod jpeg {
    include!(concat!(env!("OUT_DIR"), "/jpeg.rs"));
}

pub struct Thumbnail {
    width: usize,
    height: usize,
    buffer: Vec<u8>,
    output: *mut u8,
    output_len: libc::c_ulong,
}
impl Thumbnail {
    pub fn new(width: usize, height: usize) -> Result<Self, Error> {
        if width == 0 || height == 0 {
            return Err(Error::Contract("empty source thumbnail dimensions"));
        }
        let padded = height
            .checked_add(15)
            .map(|height| height & !15)
            .ok_or(Error::Contract("thumbnail height overflow"))?;
        let length = width
            .checked_mul(padded)
            .and_then(|len| len.checked_mul(3))
            .map(|len| len / 2)
            .ok_or(Error::Contract("thumbnail allocation overflow"))?;
        Ok(Self {
            width,
            height,
            buffer: vec![0; length],
            output: std::ptr::null_mut(),
            output_len: 0,
        })
    }
    pub fn generate(
        &mut self,
        mapping: &Mapping,
        metadata: &VisionMetadata,
    ) -> Result<Vec<u8>, Error> {
        let downscale = metadata.width / self.width;
        if downscale == 0 || downscale.checked_mul(self.height) != Some(metadata.height) {
            return Err(Error::Contract("source thumbnail downscale assertion"));
        }
        let pixels = self.width * self.height;
        for hy in 0..self.height / 2 {
            for hx in 0..self.width / 2 {
                let ix = hx * downscale + (downscale - 1) / 2;
                let iy = hy * downscale + (downscale - 1) / 2;
                for y in 0..2 {
                    for x in 0..2 {
                        let address = (iy * 2 + y) * metadata.stride + ix * 2 + x;
                        self.buffer[(hy * 2 + y) * self.width + hx * 2 + x] =
                            mapping.byte(address)?;
                    }
                }
                let address = metadata.uv_offset + iy * metadata.stride + ix * 2;
                self.buffer[pixels + hy * self.width / 2 + hx] = mapping.byte(address)?;
                self.buffer[pixels + pixels / 4 + hy * self.width / 2 + hx] =
                    mapping.byte(address + 1)?;
            }
        }
        self.compress()
    }
    fn compress(&mut self) -> Result<Vec<u8>, Error> {
        let pixels = self.width * self.height;
        let bases = [0, pixels, pixels + pixels / 4];
        let strides = [self.width, self.width / 2, self.width / 2];
        for line in (0..self.height).step_by(16) {
            for component in 0..3 {
                let rows = if component == 0 { 16 } else { 8 };
                let start = if component == 0 { line } else { line / 2 };
                let visible_width = if component == 0 {
                    self.width
                } else {
                    self.width.div_ceil(2)
                };
                let read_width = visible_width
                    .checked_add(7)
                    .map(|width| width & !7)
                    .ok_or(Error::Contract("thumbnail block width overflow"))?;
                let end = bases[component] + (start + rows - 1) * strides[component] + read_width;
                if end > self.buffer.len() {
                    return Err(Error::Contract(
                        "inherited JPEG raw-plane allocation is insufficient",
                    ));
                }
            }
        }
        let mut error = jpeg::jpeg_error_mgr::default();
        let mut info = jpeg::jpeg_compress_struct::default();
        let image_width = u32::try_from(self.width)?;
        let image_height = u32::try_from(self.height)?;
        // SAFETY: initialized ABI structs and owned raw planes remain alive
        // through this synchronous compression; libjpeg owns output allocation.
        unsafe {
            info.err = jpeg::jpeg_std_error(&mut error);
            jpeg::jpeg_CreateCompress(
                &mut info,
                jpeg::JPEG_LIB_VERSION as i32,
                std::mem::size_of::<jpeg::jpeg_compress_struct>(),
            );
            self.clear_output();
            jpeg::jpeg_mem_dest(&mut info, &mut self.output, &mut self.output_len);
            info.image_width = image_width;
            info.image_height = image_height;
            info.input_components = 3;
            jpeg::jpeg_set_defaults(&mut info);
            jpeg::jpeg_set_colorspace(&mut info, jpeg::J_COLOR_SPACE_JCS_YCbCr);
            for index in 0..3 {
                let component = &mut *info.comp_info.add(index);
                component.h_samp_factor = if index == 0 { 2 } else { 1 };
                component.v_samp_factor = if index == 0 { 2 } else { 1 };
            }
            info.raw_data_in = 1;
            jpeg::jpeg_set_quality(&mut info, 50, 1);
            jpeg::jpeg_start_compress(&mut info, 1);
            for line in (0..self.height).step_by(16) {
                let mut y = [std::ptr::null_mut(); 16];
                let mut u = [std::ptr::null_mut(); 8];
                let mut v = [std::ptr::null_mut(); 8];
                for row in 0..16 {
                    y[row] = self.buffer.as_mut_ptr().add((line + row) * self.width);
                    if row % 2 == 0 {
                        let offset = (self.width / 2) * ((row + line) / 2);
                        u[row / 2] = self.buffer.as_mut_ptr().add(pixels + offset);
                        v[row / 2] = self.buffer.as_mut_ptr().add(pixels + pixels / 4 + offset);
                    }
                }
                let mut planes = [y.as_mut_ptr(), u.as_mut_ptr(), v.as_mut_ptr()];
                jpeg::jpeg_write_raw_data(&mut info, planes.as_mut_ptr(), 16);
            }
            jpeg::jpeg_finish_compress(&mut info);
            jpeg::jpeg_destroy_compress(&mut info);
            Ok(std::slice::from_raw_parts(self.output, usize::try_from(self.output_len)?).to_vec())
        }
    }
    fn clear_output(&mut self) {
        if !self.output.is_null() {
            // SAFETY: jpeg_mem_dest allocates with malloc, and ownership is unique.
            unsafe {
                libc::free(self.output.cast());
            }
            self.output = std::ptr::null_mut();
            self.output_len = 0;
        }
    }
}
impl Drop for Thumbnail {
    fn drop(&mut self) {
        self.clear_output();
    }
}
