use crate::Error;
use ffmpeg_next::{codec, format::Pixel, frame, software::scaling, Packet};

pub struct DecodedVideo {
    pub width: u32,
    pub height: u32,
    pub planes: [Vec<u8>; 3],
    pub strides: [u32; 3],
}
pub struct SoftwareDecoder {
    codec: codec::decoder::Video,
}
impl SoftwareDecoder {
    /// # Errors
    /// Returns failure to find or open the external FFmpeg H.264 decoder.
    pub fn new() -> Result<Self, Error> {
        Ok(Self { codec: open()? })
    }
    /// # Errors
    /// Returns failure to recreate the external codec context.
    pub fn reset(&mut self) -> Result<(), Error> {
        self.codec = open()?;
        Ok(())
    }
    /// # Errors
    /// Returns codec recreation, conversion, or frame-layout failures.
    pub fn decode(&mut self, packet: &[u8]) -> Result<Option<DecodedVideo>, Error> {
        if self.codec.send_packet(&Packet::copy(packet)).is_err() {
            self.reset()?;
            return Ok(None);
        }
        let mut last = None;
        loop {
            let mut decoded = frame::Video::empty();
            match self.codec.receive_frame(&mut decoded) {
                Ok(()) => last = Some(decoded),
                Err(ffmpeg_next::Error::Other { errno }) if errno == libc::EAGAIN => break,
                Err(ffmpeg_next::Error::Eof) => break,
                Err(_) => {
                    self.reset()?;
                    return Ok(None);
                }
            }
        }
        let Some(mut decoded) = last else {
            return Ok(None);
        };
        let reformatted = decoded.format() != Pixel::YUV420P;
        if reformatted {
            let mut convert = scaling::Context::get(
                decoded.format(),
                decoded.width(),
                decoded.height(),
                Pixel::YUV420P,
                decoded.width(),
                decoded.height(),
                scaling::Flags::BILINEAR,
            )
            .map_err(|error| failure("decoder reformat", error))?;
            let mut output = frame::Video::empty();
            convert
                .run(&decoded, &mut output)
                .map_err(|error| failure("decoder scale", error))?;
            decoded = output;
        }
        if decoded.planes() < 3 {
            return Ok(None);
        }
        let mut strides = [
            u32::try_from(decoded.stride(0))?,
            u32::try_from(decoded.stride(1))?,
            u32::try_from(decoded.stride(2))?,
        ];
        let mut planes = std::array::from_fn(|index| decoded.data(index).to_vec());
        if reformatted {
            for plane in 0..3 {
                let width = if plane == 0 {
                    decoded.width()
                } else {
                    decoded.width().div_ceil(2)
                };
                let height = if plane == 0 {
                    decoded.height()
                } else {
                    decoded.height().div_ceil(2)
                };
                let stride = width
                    .checked_add(15)
                    .ok_or(Error::Contract("decoder row alignment overflow"))?
                    / 16
                    * 16;
                let row_bytes = usize::try_from(width)?;
                let rows = usize::try_from(height)?;
                let source_stride = usize::try_from(strides[plane])?;
                let output_stride = usize::try_from(stride)?;
                let mut output = vec![
                    0;
                    output_stride
                        .checked_mul(rows)
                        .ok_or(Error::Contract("decoder plane size overflow"))?
                ];
                for row in 0..rows {
                    output[row * output_stride..row * output_stride + row_bytes].copy_from_slice(
                        &planes[plane][row * source_stride..row * source_stride + row_bytes],
                    );
                }
                planes[plane] = output;
                strides[plane] = stride;
            }
        }
        Ok(Some(DecodedVideo {
            width: decoded.width(),
            height: decoded.height(),
            planes,
            strides,
        }))
    }
}
fn open() -> Result<codec::decoder::Video, Error> {
    let codec = codec::decoder::find(codec::Id::H264)
        .ok_or(Error::Contract("external H264 decoder missing"))?;
    let mut context = codec::Context::new_with_codec(codec);
    context.set_threading(codec::threading::Config::kind(
        codec::threading::Type::Slice,
    ));
    context
        .decoder()
        .video()
        .map_err(|error| failure("decoder open", error))
}
fn failure(operation: &'static str, error: ffmpeg_next::Error) -> Error {
    Error::Ffmpeg {
        operation,
        code: error.into(),
    }
}
