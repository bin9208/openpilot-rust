#![allow(unsafe_code)]
use super::{platform, yuv, Mapping};
use crate::{config::Codec, Error};
use ffmpeg_next::codec::packet::Mut;
use ffmpeg_next::{ffi, Packet};
use openpilot_msgq::VisionMetadata;
use std::ptr::NonNull;

pub struct Software {
    frame: NonNull<ffi::AVFrame>,
    context: Option<NonNull<ffi::AVCodecContext>>,
    input: (i32, i32),
    output: (i32, i32),
    convert: Vec<u8>,
    downscale: Vec<u8>,
    service: &'static str,
    debug: bool,
    pub segment: i32,
    pub counter: i32,
}
impl Software {
    pub fn new(
        input: (i32, i32),
        output: (i32, i32),
        service: &'static str,
    ) -> Result<Self, Error> {
        if [input.0, input.1, output.0, output.1]
            .iter()
            .any(|&value| value <= 0 || value % 2 != 0)
        {
            return Err(Error::Contract(
                "software encoder requires positive even dimensions",
            ));
        }
        let convert = vec![0; yuv::size(input.0, input.1)?];
        let downscale = if input != output {
            vec![0; yuv::size(output.0, output.1)?]
        } else {
            Vec::new()
        };
        // SAFETY: libavutil allocates the AVFrame; this owner frees it once.
        let frame = NonNull::new(unsafe { ffi::av_frame_alloc() })
            .ok_or(Error::Contract("AVFrame allocation"))?;
        // SAFETY: the uniquely owned AVFrame is initialized before any codec use.
        unsafe {
            let value = &mut *frame.as_ptr();
            value.format = ffi::AVPixelFormat::AV_PIX_FMT_YUV420P as i32;
            value.width = output.0;
            value.height = output.1;
            value.linesize[..3].copy_from_slice(&[output.0, output.0 / 2, output.0 / 2]);
        }
        Ok(Self {
            frame,
            context: None,
            input,
            output,
            convert,
            downscale,
            service,
            debug: platform::debug_encoder() != 0,
            segment: -1,
            counter: 0,
        })
    }
    pub fn open(&mut self, kind: Codec, fps: i32) -> Result<(), Error> {
        if self.context.is_some() {
            return Err(Error::Contract("software encoder already open"));
        }
        let id = if kind == Codec::QcameraH264 {
            ffi::AVCodecID::AV_CODEC_ID_H264
        } else {
            ffi::AVCodecID::AV_CODEC_ID_FFVHUFF
        };
        // SAFETY: codec lookup returns a library-owned descriptor; the allocated
        // context stays private and only the source's four fields are changed.
        unsafe {
            let codec = ffi::avcodec_find_encoder(id);
            if codec.is_null() {
                return Err(Error::Contract("required external FFmpeg encoder missing"));
            }
            let pointer = NonNull::new(ffi::avcodec_alloc_context3(codec))
                .ok_or(Error::Contract("AVCodecContext allocation"))?;
            let context = &mut *pointer.as_ptr();
            context.width = self.output.0;
            context.height = self.output.1;
            context.pix_fmt = ffi::AVPixelFormat::AV_PIX_FMT_YUV420P;
            context.time_base = ffi::AVRational { num: 1, den: fps };
            let code = ffi::avcodec_open2(pointer.as_ptr(), codec, std::ptr::null_mut());
            if code < 0 {
                let mut raw = pointer.as_ptr();
                ffi::avcodec_free_context(&mut raw);
                return Err(Error::Ffmpeg {
                    operation: "open",
                    code,
                });
            }
            self.context = Some(pointer);
        }
        self.segment = self.segment.wrapping_add(1);
        self.counter = 0;
        Ok(())
    }
    pub fn close(&mut self) {
        if let Some(context) = self.context.take() {
            // SAFETY: the owned context is freed without a flush, as in source.
            unsafe {
                let mut raw = context.as_ptr();
                ffi::avcodec_free_context(&mut raw);
            }
        }
    }
    pub fn encode(
        &mut self,
        mapping: &Mapping,
        metadata: &VisionMetadata,
        mut publish: impl FnMut(i32, u32, u32, &[u8]) -> Result<(), Error>,
        mut diagnostic: impl FnMut(&str, i32),
    ) -> Result<i32, Error> {
        if self.input
            != (
                i32::try_from(metadata.width)?,
                i32::try_from(metadata.height)?,
            )
        {
            return Err(Error::Contract("software encoder input dimensions changed"));
        }
        let context = self
            .context
            .ok_or(Error::Contract("software encoder not open"))?;
        yuv::convert(mapping, metadata, &mut self.convert)?;
        if !self.downscale.is_empty() {
            yuv::scale(&self.convert, self.input, &mut self.downscale, self.output)?;
        }
        let pixels = usize::try_from(self.output.0)? * usize::try_from(self.output.1)?;
        let data = if self.downscale.is_empty() {
            &mut self.convert
        } else {
            &mut self.downscale
        };
        let mut result = self.counter;
        // SAFETY: owned I420 storage remains stable for this synchronous send.
        // No AVBufferRef is installed, preserving source's non-refcounted frame.
        unsafe {
            let frame = &mut *self.frame.as_ptr();
            frame.data[0] = data.as_mut_ptr();
            frame.data[1] = data.as_mut_ptr().add(pixels);
            frame.data[2] = data.as_mut_ptr().add(pixels + pixels / 4);
            frame.pts = i64::from(self.counter.wrapping_mul(50).wrapping_mul(1000));
            let code = ffi::avcodec_send_frame(context.as_ptr(), self.frame.as_ptr());
            if code < 0 {
                diagnostic("avcodec_send_frame", code);
                result = -1;
            }
        }
        let mut packet = Packet::empty();
        while result >= 0 {
            // SAFETY: both FFmpeg objects are live, exclusively used by this thread.
            let code =
                unsafe { ffi::avcodec_receive_packet(context.as_ptr(), packet.as_mut_ptr()) };
            if code == ffi::AVERROR_EOF {
                break;
            }
            if code == -libc::EAGAIN {
                result = 0;
                break;
            }
            if code < 0 {
                diagnostic("avcodec_receive_packet", code);
                result = -1;
                break;
            }
            let flags = if packet.is_key() { 8 } else { 0 };
            if self.debug {
                println!(
                    "{:>20} got {:8} bytes flags {:8x} idx {:4} id {:8}",
                    self.service,
                    packet.size(),
                    packet.flags().bits(),
                    self.counter,
                    metadata.frame_id as i32
                );
            }
            publish(
                self.segment,
                self.counter as u32,
                flags,
                packet.data().unwrap_or(&[]),
            )?;
            self.counter = self.counter.wrapping_add(1);
        }
        Ok(result)
    }
}
impl Drop for Software {
    fn drop(&mut self) {
        self.close();
        // SAFETY: this is the only AVFrame owner, and codecs are already closed.
        unsafe {
            let mut frame = self.frame.as_ptr();
            ffi::av_frame_free(&mut frame);
        }
    }
}
