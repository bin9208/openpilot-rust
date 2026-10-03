use super::{platform, publisher::VideoPublisher, software::Software, v4l::V4l, Mapping};
use crate::{profile::EncoderInfo, Error};
use openpilot_logging::{log_site, record::Level};
use openpilot_msgq::VisionMetadata;
use std::sync::Arc;

pub enum Video {
    Software {
        codec: Software,
        publisher: VideoPublisher,
    },
    V4l(V4l),
}
impl Video {
    pub fn new(info: &EncoderInfo, input: (i32, i32)) -> Result<Self, Error> {
        let pc = !cfg!(feature = "visionipc-ion");
        let settings = info
            .quality
            .settings(input.0, pc, platform::stream_bitrate());
        let mut dimensions = info.dimensions(input.0, input.1, settings);
        if !pc && info.publish == "livestreamRoadEncodeData" {
            dimensions = (964, 604);
        }
        let publisher = VideoPublisher::new(info.publish, dimensions, settings.codec)?;
        if pc {
            let mut codec = Software::new(input, dimensions, info.publish)?;
            codec.open(settings.codec, info.fps)?;
            Ok(Self::Software { codec, publisher })
        } else {
            Ok(Self::V4l(V4l::new(
                info, input, dimensions, settings, publisher,
            )?))
        }
    }
    pub fn idle(&self, idle: bool) {
        if let Self::V4l(codec) = self {
            codec.idle(idle);
        }
    }
    pub fn rotate(&mut self, info: &EncoderInfo) -> Result<(), Error> {
        match self {
            Self::Software { codec, .. } => {
                codec.close();
                let settings = info.quality.settings(0, true, platform::stream_bitrate());
                codec.open(settings.codec, info.fps)
            }
            Self::V4l(codec) => codec.rotate(),
        }
    }
    pub fn encode(
        &mut self,
        mapping: Arc<Mapping>,
        length: usize,
        extra: &VisionMetadata,
    ) -> Result<i32, Error> {
        match self {
            Self::Software { codec, publisher } => codec.encode(
                &mapping,
                extra,
                |segment, index, flags, bytes| {
                    publisher.publish(segment, index, extra, flags, &[], bytes)
                },
                |operation, code| {
                    platform::emit(
                        log_site!(),
                        Level::Error,
                        format!("{operation} error {code}"),
                    );
                },
            ),
            Self::V4l(codec) => codec.encode(mapping, length, extra),
        }
    }
}
