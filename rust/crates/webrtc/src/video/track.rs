use super::{
    debug::DebugTrack,
    ipc::{Camera, CameraTrack},
};
use crate::Error;

pub(crate) enum Payload {
    H264(Vec<u8>),
    Encoded(Vec<Vec<u8>>),
}

pub(crate) struct Frame {
    pub source_id: Option<u32>,
    pub pts: i64,
    pub payload: Payload,
}

pub(crate) enum Track {
    Camera(CameraTrack),
    Debug(DebugTrack),
}

impl Track {
    pub fn new(camera: Camera, carrot: bool, debug: bool) -> Result<Self, Error> {
        if debug {
            Ok(Self::Debug(DebugTrack::new()?))
        } else {
            Ok(Self::Camera(CameraTrack::for_runtime(camera, carrot)?))
        }
    }

    pub const fn is_debug(&self) -> bool {
        matches!(self, Self::Debug(_))
    }

    pub fn receive(&mut self, mime: &str) -> Result<Option<Frame>, Error> {
        match self {
            Self::Camera(track) => Ok(track.receive()?.map(|frame| Frame {
                source_id: Some(frame.frame_id),
                pts: frame.pts,
                payload: Payload::H264(frame.data),
            })),
            Self::Debug(track) => Ok(track.receive(mime)?.map(|frame| Frame {
                source_id: None,
                pts: frame.pts,
                payload: Payload::Encoded(frame.payloads),
            })),
        }
    }

    pub fn keyframe(&mut self) {
        if let Self::Debug(track) = self {
            track.keyframe();
        }
    }

    pub fn bitrate(&mut self, bitrate: u32) {
        if let Self::Debug(track) = self {
            track.bitrate(bitrate);
        }
    }
}
