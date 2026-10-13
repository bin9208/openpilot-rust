use super::{Error, Result};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InputFormat {
    #[default]
    Auto,
    Nv12,
}
#[derive(Clone, Debug)]
pub struct Config {
    pub width: i32,
    pub height: i32,
    pub fps: i32,
    pub bitrate: i32,
    pub gop: i32,
    pub slice_max_bytes: i32,
    pub rate_control: i32,
    pub realtime_priority: bool,
    pub debug: bool,
    pub input_format: InputFormat,
    pub device_path: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            fps: 30,
            bitrate: 1_000_000,
            gop: 30,
            slice_max_bytes: 4096,
            rate_control: 2,
            realtime_priority: false,
            debug: false,
            input_format: InputFormat::Auto,
            device_path: "/dev/v4l/by-path/platform-aa00000.qcom_vidc-video-index1".into(),
        }
    }
}
impl Config {
    pub(super) fn validate(&self) -> Result<()> {
        let error = if self.width <= 0 || self.height <= 0 {
            "cluster H264 encoder dimensions must be positive"
        } else if self.width % 2 != 0 || self.height % 2 != 0 {
            "cluster H264 encoder requires even dimensions"
        } else if self.fps <= 0 {
            "cluster H264 encoder fps must be positive"
        } else if self.bitrate <= 0 {
            "cluster H264 encoder bitrate must be positive"
        } else if self.gop <= 0 {
            "cluster H264 encoder gop must be positive"
        } else if self.slice_max_bytes < 0 {
            "cluster H264 encoder slice max bytes must be 0 or greater"
        } else if !(0..=7).contains(&self.rate_control) {
            "cluster H264 encoder rate control is invalid"
        } else if self.device_path.is_empty() {
            "cluster H264 encoder device path must not be empty"
        } else {
            return Ok(());
        };
        Err(Error::Contract(error))
    }
}
#[derive(Clone, Debug)]
pub struct Packet {
    pub data: Vec<u8>,
    pub flags: u32,
    pub timestamp_us: u64,
    pub codec_config: bool,
    pub keyframe: bool,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Timings {
    pub pre_poll_us: u64,
    pub wait_input_us: u64,
    pub convert_us: u64,
    pub sync_us: u64,
    pub queue_us: u64,
    pub post_poll_us: u64,
    pub total_us: u64,
}
