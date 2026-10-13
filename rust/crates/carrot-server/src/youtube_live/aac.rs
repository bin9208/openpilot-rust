use crate::Error;
use ffmpeg_next::{self as av, ffi, format::sample::Type, ChannelLayout};
use std::{marker::PhantomData, ptr, rc::Rc, slice};

pub(super) const RATE: i32 = 44_100;
pub(super) const SAMPLES: usize = 1024;
pub(super) struct Packet {
    pub timestamp_ms: i64,
    pub payload: Vec<u8>,
}
pub(super) struct Audio {
    opened: av::codec::encoder::audio::Encoder,
    pub configuration: Vec<u8>,
    owner: PhantomData<Rc<()>>,
}
fn failure(error: av::Error) -> Error {
    Error::Source(error.to_string())
}
impl Audio {
    pub fn new() -> Result<Self, Error> {
        av::init().map_err(failure)?;
        let codec = av::encoder::find(av::codec::Id::AAC)
            .ok_or_else(|| failure(av::Error::EncoderNotFound))?;
        let mut encoder = context(codec)?.encoder().audio().map_err(failure)?;
        encoder.set_rate(RATE);
        encoder.set_channel_layout(ChannelLayout::STEREO);
        encoder.set_channels(2);
        encoder.set_format(av::format::Sample::F32(Type::Planar));
        encoder.set_bit_rate(128_000);
        encoder.set_time_base((1, RATE));
        let opened = encoder.open_as(codec).map_err(failure)?;
        let configuration = configuration(&opened)?;
        Ok(Self {
            opened,
            configuration,
            owner: PhantomData,
        })
    }
    pub fn silence(&mut self, pts: i64) -> Result<Vec<Packet>, Error> {
        let mut frame = silence_frame()?;
        frame.set_pts(Some(pts));
        self.opened.send_frame(&frame).map_err(failure)?;
        self.packets()
    }
    pub fn finish(&mut self) -> Result<Vec<Packet>, Error> {
        self.opened.send_eof().map_err(failure)?;
        self.packets()
    }
    fn packets(&mut self) -> Result<Vec<Packet>, Error> {
        let mut output = Vec::new();
        loop {
            let mut packet = av::Packet::empty();
            match self.opened.receive_packet(&mut packet) {
                Ok(()) => {
                    let timestamp_ms =
                        packet.pts().unwrap_or(0).saturating_mul(1000) / i64::from(RATE);
                    output.push(Packet {
                        timestamp_ms: timestamp_ms.max(0),
                        payload: packet.data().unwrap_or_default().to_vec(),
                    });
                }
                Err(
                    av::Error::Eof
                    | av::Error::Other {
                        errno: libc::EAGAIN,
                    },
                ) => break,
                Err(error) => return Err(failure(error)),
            }
        }
        Ok(output)
    }
}
#[expect(
    unsafe_code,
    reason = "Check nullable codec allocation before transferring ownership to the FFmpeg wrapper"
)]
fn context(codec: av::Codec) -> Result<av::codec::Context, Error> {
    // SAFETY: The registered AAC codec pointer lives for the library lifetime.
    // The checked context is transferred once with no external owner, so the
    // wrapper destructor frees it on every later success/error path.
    unsafe {
        let pointer = ffi::avcodec_alloc_context3(codec.as_ptr());
        if pointer.is_null() {
            return Err(failure(av::Error::from(-libc::ENOMEM)));
        }
        Ok(av::codec::Context::wrap(pointer, None))
    }
}
#[expect(
    unsafe_code,
    reason = "Copy owned AAC extradata while the live codec context owns its initialized allocation"
)]
fn configuration(codec: &av::codec::encoder::audio::Encoder) -> Result<Vec<u8>, Error> {
    // SAFETY: The opened codec owns extradata until its destructor; the shared
    // codec borrow excludes mutation while the validated byte slice is copied.
    unsafe {
        let context = codec.as_ptr();
        let size = usize::try_from((*context).extradata_size)
            .map_err(|_| failure(av::Error::InvalidData))?;
        if size == 0 {
            return Ok(vec![0x12, 0x10]);
        }
        if (*context).extradata.is_null() {
            return Err(failure(av::Error::InvalidData));
        }
        Ok(slice::from_raw_parts((*context).extradata, size).to_vec())
    }
}
#[expect(
    unsafe_code,
    reason = "Check FFmpeg frame/buffer allocations and initialize every aligned planar sample byte"
)]
fn silence_frame() -> Result<av::frame::Audio, Error> {
    // SAFETY: The checked fresh AVFrame is transferred exactly once into Audio.
    // FFmpeg allocates both planar buffers; their pointers and shared linesize
    // are validated before initialization, and Audio releases them on error.
    unsafe {
        let raw = ffi::av_frame_alloc();
        if raw.is_null() {
            return Err(failure(av::Error::from(-libc::ENOMEM)));
        }
        let mut frame = av::frame::Audio::wrap(raw);
        frame.set_format(av::format::Sample::F32(Type::Planar));
        frame.set_samples(SAMPLES);
        frame.set_rate(u32::try_from(RATE).map_err(|_| failure(av::Error::InvalidData))?);
        frame.set_channel_layout(ChannelLayout::STEREO);
        ffi::av_channel_layout_default(&mut (*raw).ch_layout, 2);
        let result = ffi::av_frame_get_buffer(raw, 0);
        if result < 0 {
            return Err(failure(av::Error::from(result)));
        }
        let size =
            usize::try_from((*raw).linesize[0]).map_err(|_| failure(av::Error::InvalidData))?;
        if size < SAMPLES * std::mem::size_of::<f32>() {
            return Err(failure(av::Error::InvalidData));
        }
        for plane in 0..2 {
            if (*raw).data[plane].is_null() {
                return Err(failure(av::Error::InvalidData));
            }
            ptr::write_bytes((*raw).data[plane], 0, size);
        }
        Ok(frame)
    }
}
