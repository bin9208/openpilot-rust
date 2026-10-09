use super::pack;
use crate::Error;
use ffmpeg_next::{codec, encoder, format::Pixel, frame, picture, Dictionary, Packet, Rescale};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Codec {
    Vp8,
    H264,
}

impl Codec {
    pub(super) fn parse(mime: &str) -> Result<Self, Error> {
        if mime.eq_ignore_ascii_case("video/VP8") {
            Ok(Self::Vp8)
        } else if mime.eq_ignore_ascii_case("video/H264") {
            Ok(Self::H264)
        } else {
            Err(Error::Contract("unsupported generated video codec"))
        }
    }

    pub(super) fn bitrate(self, requested: Option<u32>) -> u32 {
        match self {
            Self::Vp8 => match requested {
                Some(value) => value.clamp(250_000, 1_500_000),
                None => 500_000,
            },
            Self::H264 => match requested {
                Some(value) => value.clamp(500_000, 3_000_000),
                None => 1_000_000,
            },
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Work {
    pub pts: i64,
    pub keyframe: bool,
    pub bitrate: Option<u32>,
}

pub(super) struct Output {
    pub pts: i64,
    pub payloads: Vec<Vec<u8>>,
}

pub(super) struct Encoder {
    kind: Codec,
    codec: Option<encoder::Video>,
    bitrate: u32,
    opened_bitrate: u32,
    picture_id: u16,
}

impl Encoder {
    pub(super) fn new(kind: Codec) -> Self {
        let random = uuid::Uuid::new_v4();
        Self {
            kind,
            codec: None,
            bitrate: kind.bitrate(None),
            opened_bitrate: kind.bitrate(None),
            picture_id: u16::from_be_bytes([random.as_bytes()[0], random.as_bytes()[1]]) & 0x7fff,
        }
    }

    fn open(&self) -> Result<encoder::Video, Error> {
        let name = match self.kind {
            Codec::Vp8 => "libvpx",
            Codec::H264 => "libx264",
        };
        let descriptor = encoder::find_by_name(name).ok_or(ffmpeg_next::Error::EncoderNotFound)?;
        let mut codec = codec::context::Context::new_with_codec(descriptor)
            .encoder()
            .video()?;
        codec.set_width(640);
        codec.set_height(480);
        codec.set_format(Pixel::YUV420P);
        codec.set_bit_rate(usize::try_from(self.bitrate)?);
        let mut options = Dictionary::new();
        match self.kind {
            Codec::Vp8 => {
                codec.set_time_base((1, 1_000_000));
                codec.set_gop(3000);
                codec.set_qmin(2);
                codec.set_qmax(56);
                codec.set_threading(codec::threading::Config::count(1));
                for name in ["bufsize", "minrate", "maxrate"] {
                    options.set(name, &self.bitrate.to_string());
                }
                for (name, value) in [
                    ("cpu-used", "-6"),
                    ("deadline", "realtime"),
                    ("lag-in-frames", "0"),
                    ("noise-sensitivity", "4"),
                    ("overshoot-pct", "15"),
                    ("partitions", "0"),
                    ("static-thresh", "1"),
                    ("undershoot-pct", "100"),
                ] {
                    options.set(name, value);
                }
            }
            Codec::H264 => {
                codec.set_time_base((1, 30));
                codec.set_frame_rate(Some((30, 1)));
                options.set("level", "31");
                options.set("tune", "zerolatency");
                options.set("profile", "baseline");
            }
        }
        Ok(codec.open_with(options)?)
    }

    pub(super) fn encode(&mut self, work: Work) -> Result<Output, Error> {
        let requested = self.kind.bitrate(work.bitrate);
        if self.codec.is_some()
            && requested.abs_diff(self.opened_bitrate) * 10 > self.opened_bitrate
        {
            self.codec = None;
        }
        self.bitrate = requested;
        if self.codec.is_none() {
            self.codec = Some(self.open()?);
            self.opened_bitrate = self.bitrate;
        }
        let codec = self
            .codec
            .as_mut()
            .ok_or(Error::Contract("debug codec missing"))?;
        let mut frame = frame::Video::new(Pixel::YUV420P, 640, 480);
        for index in 0..frame.planes() {
            frame.data_mut(index).fill(0);
        }
        frame.set_kind(if work.keyframe {
            picture::Type::I
        } else {
            picture::Type::None
        });
        let frame_pts = work.pts.rescale((1, 90_000), codec.time_base());
        frame.set_pts(Some(frame_pts));
        codec.send_frame(&frame)?;
        let mut data = Vec::new();
        loop {
            let mut packet = Packet::empty();
            match codec.receive_packet(&mut packet) {
                Ok(()) => data
                    .extend_from_slice(packet.data().ok_or(Error::Contract("empty codec packet"))?),
                Err(ffmpeg_next::Error::Other { errno }) if errno == ffmpeg_next::error::EAGAIN => {
                    break
                }
                Err(error) => return Err(error.into()),
            }
        }
        let payloads = match self.kind {
            Codec::H264 => pack(&data).map_err(|_| Error::Contract("invalid debug H264"))?,
            Codec::Vp8 => vp8(&data, self.picture_id),
        };
        self.picture_id = self.picture_id.wrapping_add(1) & 0x7fff;
        let base = codec.time_base();
        let pts = frame_pts * i64::from(base.numerator()) * 90_000 / i64::from(base.denominator());
        Ok(Output { pts, payloads })
    }
}

fn vp8(data: &[u8], picture_id: u16) -> Vec<Vec<u8>> {
    let mut descriptor = vec![0x90, 0x80];
    if picture_id < 128 {
        descriptor.push(picture_id.to_be_bytes()[1]);
    } else {
        descriptor.extend_from_slice(&(picture_id | 0x8000).to_be_bytes());
    }
    let mut result = Vec::new();
    for chunk in data.chunks(1300 - descriptor.len()) {
        let mut payload = descriptor.clone();
        payload.extend_from_slice(chunk);
        result.push(payload);
        descriptor[0] = 0x80;
    }
    result
}
