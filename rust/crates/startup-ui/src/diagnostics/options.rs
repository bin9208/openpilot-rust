use crate::Error;
use std::path::PathBuf;
#[derive(Clone, Debug)]
pub struct Options {
    pub show_fps: bool,
    pub show_touches: bool,
    pub strict: bool,
    pub grid: i32,
    pub profile_frames: u64,
    pub profile_stats: usize,
    pub profile_startup: bool,
    pub burn_in: bool,
    pub record: bool,
    pub record_output: PathBuf,
    pub record_quality: u8,
    pub record_bitrate: String,
    pub record_speed: u32,
}
impl Options {
    pub fn from_environment() -> Result<Self, Error> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, Error> {
        let flag = |key| get(key).as_deref() == Some("1");
        let mut output = PathBuf::from(get("RECORD_OUTPUT").unwrap_or_else(|| "output".into()));
        output.set_extension("mp4");
        let options = Self {
            show_fps: flag("SHOW_FPS"),
            show_touches: flag("SHOW_TOUCHES"),
            strict: flag("STRICT_MODE"),
            grid: get("GRID")
                .unwrap_or_else(|| "0".into())
                .parse()
                .map_err(|_| Error::Contract("invalid GRID"))?,
            profile_frames: get("PROFILE_RENDER")
                .unwrap_or_else(|| "0".into())
                .parse()
                .map_err(|_| Error::Contract("invalid PROFILE_RENDER"))?,
            profile_stats: get("PROFILE_STATS")
                .unwrap_or_else(|| "100".into())
                .parse()
                .map_err(|_| Error::Contract("invalid PROFILE_STATS"))?,
            profile_startup: get("PROFILE_STARTUP").is_some(),
            burn_in: get("BURN_IN").is_some(),
            record: flag("RECORD"),
            record_output: output,
            record_quality: get("RECORD_QUALITY")
                .unwrap_or_else(|| "23".into())
                .parse()
                .map_err(|_| Error::Contract("invalid RECORD_QUALITY"))?,
            record_bitrate: get("RECORD_BITRATE").unwrap_or_default(),
            record_speed: get("RECORD_SPEED")
                .unwrap_or_else(|| "1".into())
                .parse()
                .map_err(|_| Error::Contract("invalid RECORD_SPEED"))?,
        };
        if options.record_quality > 51 || options.record_speed == 0 {
            return Err(Error::Contract("invalid recording quality or speed"));
        }
        Ok(options)
    }
}
impl Default for Options {
    fn default() -> Self {
        Self {
            show_fps: false,
            show_touches: false,
            strict: false,
            grid: 0,
            profile_frames: 0,
            profile_stats: 100,
            profile_startup: false,
            burn_in: false,
            record: false,
            record_output: "output.mp4".into(),
            record_quality: 23,
            record_bitrate: String::new(),
            record_speed: 1,
        }
    }
}
