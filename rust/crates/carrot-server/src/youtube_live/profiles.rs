use crate::Value;

#[derive(Clone, Copy)]
pub struct Profile {
    pub quality: u8,
    pub label: &'static str,
    pub process: &'static str,
    pub encoder_flag: &'static str,
    pub width: u16,
    pub height: u16,
    pub video_kbps: u16,
}
pub const SOURCE: &str = "youtubeRoadEncodeData";
pub const FPS: u32 = 20;
pub const PROFILES: [Profile; 4] = [
    Profile {
        quality: 0,
        label: "low",
        process: "youtube_low_encoderd",
        encoder_flag: "--youtube-low",
        width: 854,
        height: 480,
        video_kbps: 750,
    },
    Profile {
        quality: 1,
        label: "medium",
        process: "youtube_medium_encoderd",
        encoder_flag: "--youtube-medium",
        width: 1280,
        height: 720,
        video_kbps: 2000,
    },
    Profile {
        quality: 2,
        label: "high",
        process: "youtube_encoderd",
        encoder_flag: "--youtube",
        width: 1920,
        height: 1080,
        video_kbps: 4200,
    },
    Profile {
        quality: 3,
        label: "wide",
        process: "youtube_wide_encoderd",
        encoder_flag: "--youtube-wide",
        width: 1280,
        height: 720,
        video_kbps: 2700,
    },
];
pub fn selected(quality: i32) -> Profile {
    usize::try_from(quality)
        .ok()
        .and_then(|index| PROFILES.get(index))
        .copied()
        .unwrap_or(PROFILES[0])
}
impl Profile {
    pub fn target(self) -> Value {
        Value::object([
            ("width", Value::integer(self.width)),
            ("height", Value::integer(self.height)),
            ("fps", Value::integer(FPS)),
            ("video_kbps", Value::integer(self.video_kbps)),
            ("gop_seconds", Value::integer(2)),
        ])
    }
}
