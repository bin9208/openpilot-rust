use crate::config::{Camera, Mode, Quality, Settings};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Recording {
    pub road: bool,
    pub wide: bool,
    pub front: bool,
    pub audio: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct EncoderInfo {
    pub publish: &'static str,
    pub thumbnail: Option<&'static str>,
    pub filename: Option<&'static str>,
    pub record: bool,
    pub include_audio: bool,
    pub width: i32,
    pub height: i32,
    pub fps: i32,
    pub quality: Quality,
}
impl EncoderInfo {
    pub fn dimensions(&self, width: i32, height: i32, settings: Settings) -> (i32, i32) {
        let select = |configured, fallback, input| {
            if configured > 0 {
                configured
            } else if fallback > 0 {
                fallback
            } else {
                input
            }
        };
        (
            select(settings.width, self.width, width),
            select(settings.height, self.height, height),
        )
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct CameraInfo {
    pub thread: &'static str,
    pub camera: Camera,
    pub encoders: Vec<EncoderInfo>,
}

pub fn cameras(mode: Mode, recording: Recording) -> Vec<CameraInfo> {
    mode.cameras()
        .iter()
        .map(|&camera| {
            let thread = match (mode, camera) {
                (Mode::CarrotVision, _) => "carrot_vision_road_encoder",
                (Mode::YoutubeLow, _) => "youtube_road_low_encoder",
                (Mode::YoutubeMedium, _) => "youtube_road_medium_encoder",
                (Mode::Youtube, _) => "youtube_road_encoder",
                (Mode::YoutubeWide, _) => "youtube_wide_road_encoder",
                (_, Camera::Road) => "road_cam_encoder",
                (_, Camera::Driver) => "driver_cam_encoder",
                (_, Camera::WideRoad) => "wide_road_cam_encoder",
            };
            let (publish, quality) = match (mode, camera) {
                (Mode::Main, Camera::Road) => ("roadEncodeData", Quality::Main),
                (Mode::Main, Camera::Driver) => ("driverEncodeData", Quality::Main),
                (Mode::Main, Camera::WideRoad) => ("wideRoadEncodeData", Quality::Main),
                (Mode::Stream | Mode::CarrotVision, Camera::Road) => {
                    ("livestreamRoadEncodeData", Quality::Stream)
                }
                (Mode::Stream | Mode::CarrotVision, Camera::Driver) => {
                    ("livestreamDriverEncodeData", Quality::Stream)
                }
                (Mode::Stream | Mode::CarrotVision, Camera::WideRoad) => {
                    ("livestreamWideRoadEncodeData", Quality::Stream)
                }
                (Mode::YoutubeLow, _) => ("youtubeRoadEncodeData", Quality::YoutubeLow),
                (Mode::YoutubeMedium, _) => ("youtubeRoadEncodeData", Quality::YoutubeMedium),
                (Mode::Youtube, _) => ("youtubeRoadEncodeData", Quality::Youtube),
                (Mode::YoutubeWide, _) => ("youtubeRoadEncodeData", Quality::YoutubeWide),
            };
            let mut info = encoder(publish, quality);
            if mode == Mode::Main {
                let (filename, record) = match camera {
                    Camera::Road => ("fcamera.hevc", recording.road),
                    Camera::Driver => ("dcamera.hevc", recording.front),
                    Camera::WideRoad => ("ecamera.hevc", recording.wide),
                };
                info.filename = Some(filename);
                info.record = record;
                if camera == Camera::Road {
                    info.thumbnail = Some("thumbnail");
                }
            }
            if mode == Mode::Youtube {
                info.width = 1920;
                info.height = 1080;
            }
            let mut encoders = vec![info];
            if mode == Mode::Main && camera == Camera::Road {
                let mut qcam = encoder("qRoadEncodeData", Quality::Qcam);
                qcam.filename = Some("qcamera.ts");
                qcam.record = true;
                qcam.include_audio = recording.audio;
                qcam.width = 526;
                qcam.height = 330;
                encoders.push(qcam);
            }
            CameraInfo {
                thread,
                camera,
                encoders,
            }
        })
        .collect()
}
fn encoder(publish: &'static str, quality: Quality) -> EncoderInfo {
    EncoderInfo {
        publish,
        thumbnail: None,
        filename: None,
        record: false,
        include_audio: false,
        width: -1,
        height: -1,
        fps: 20,
        quality,
    }
}
