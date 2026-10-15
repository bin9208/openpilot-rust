use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Mode {
    Main,
    Stream,
    CarrotVision,
}
impl Mode {
    pub fn parse(argument: Option<&str>) -> Option<Self> {
        match argument {
            None => Some(Self::Main),
            Some("--stream") => Some(Self::Stream),
            Some("--carrot-vision-road") => Some(Self::CarrotVision),
            Some(_) => None,
        }
    }
    pub fn cameras(self) -> &'static [Camera] {
        match self {
            Self::Main | Self::Stream => &[Camera::Road, Camera::Driver, Camera::WideRoad],
            Self::CarrotVision => &[Camera::Road],
        }
    }
    pub fn schedule(self, pc: bool) -> Scheduling {
        Scheduling {
            priority: (!pc && self != Self::CarrotVision).then_some(52),
            core: (!pc).then_some(if self == Self::CarrotVision { 0 } else { 3 }),
        }
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Camera {
    Road,
    Driver,
    WideRoad,
}
impl Camera {
    pub const fn index(self) -> usize {
        match self {
            Self::Road => 0,
            Self::Driver => 1,
            Self::WideRoad => 2,
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub struct Scheduling {
    pub priority: Option<i32>,
    pub core: Option<usize>,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Codec {
    FullHevc,
    BigBoxLossless,
    QcameraH264,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Quality {
    Main,
    Qcam,
    Stream,
}
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub struct Settings {
    pub codec: Codec,
    pub bitrate: i32,
    pub gop: i32,
    pub b_frames: i32,
    pub cbr: bool,
    pub width: i32,
    pub height: i32,
}
impl Quality {
    pub fn settings(self, input_width: i32, pc: bool, stream_bitrate: i32) -> Settings {
        let mut value = Settings {
            codec: Codec::QcameraH264,
            bitrate: 0,
            gop: 15,
            b_frames: 0,
            cbr: false,
            width: -1,
            height: -1,
        };
        match self {
            Self::Main => {
                value.codec = if pc {
                    Codec::BigBoxLossless
                } else {
                    Codec::FullHevc
                };
                value.bitrate = if input_width <= 1344 {
                    5_000_000
                } else {
                    10_000_000
                };
                value.gop = if input_width <= 1344 { 20 } else { 30 };
            }
            Self::Qcam => value.bitrate = 256_000,
            Self::Stream => value.bitrate = stream_bitrate,
        }
        value
    }
}
