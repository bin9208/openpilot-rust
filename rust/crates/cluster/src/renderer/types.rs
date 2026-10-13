use super::{
    geometry::{Anchor, Point},
    tokens::{SYSTEM_PANEL_W, SYSTEM_PANEL_Y},
};

#[derive(Clone, Copy, Debug)]
pub struct Ring {
    pub center: Point,
    pub inner: f64,
    pub outer: f64,
    pub start: f64,
    pub end: f64,
    pub segments: i32,
    pub color: [u8; 4],
}

#[derive(Clone, Copy, Debug)]
pub struct StrokeStyle {
    pub color: [u8; 4],
    pub width: i32,
    pub anchor: Anchor,
    pub cache: bool,
}
impl Default for StrokeStyle {
    fn default() -> Self {
        Self {
            color: [0, 0, 0, 255],
            width: 0,
            anchor: Anchor::Left,
            cache: false,
        }
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct SystemStats {
    pub memory_total_bytes: Option<super::linux_stats::MemoryBytes>,
    pub memory_used_bytes: Option<super::linux_stats::MemoryBytes>,
    pub memory_used_percent: Option<f64>,
    pub cpu_core_percents: Vec<Option<f64>>,
    pub cpu_used_percent: Option<f64>,
    pub disk_used_percent: Option<f64>,
}

#[derive(Clone, Copy, Debug)]
pub struct SystemPanelOptions<'a> {
    pub panel_x: Option<f64>,
    pub panel_y: f64,
    pub panel_w: f64,
    pub panel_h: Option<f64>,
    pub status_text: Option<&'a str>,
    pub show_runtime_info: bool,
    pub title_text: &'a str,
    pub stats: Option<&'a SystemStats>,
}
impl Default for SystemPanelOptions<'_> {
    fn default() -> Self {
        Self {
            panel_x: None,
            panel_y: SYSTEM_PANEL_Y,
            panel_w: SYSTEM_PANEL_W,
            panel_h: None,
            status_text: None,
            show_runtime_info: false,
            title_text: "SYSTEM",
            stats: None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FrameClock {
    pub monotonic: f64,
    pub perf_counter: f64,
    pub wall_time: f64,
}
impl FrameClock {
    /// # Errors
    /// Returns a clock value that cannot be represented as source seconds.
    pub fn now() -> super::Result<Self> {
        use num_traits::ToPrimitive;
        let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
        let seconds = time
            .tv_sec
            .to_f64()
            .ok_or(super::Error::Contract("monotonic seconds out of range"))?
            + time
                .tv_nsec
                .to_f64()
                .ok_or(super::Error::Contract("monotonic nanoseconds out of range"))?
                / 1e9;
        let wall_time = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(duration) => duration.as_secs_f64(),
            Err(error) => -error.duration().as_secs_f64(),
        };
        Ok(Self {
            monotonic: seconds,
            perf_counter: seconds,
            wall_time,
        })
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Options {
    pub width: i32,
    pub height: i32,
    pub title: String,
    pub target_fps: i32,
    pub theme_mode: String,
    pub screen_mode: i64,
    pub panel_layout: i64,
    pub language: String,
    pub is_metric: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 480,
            title: "carrotpilot cluster".into(),
            target_fps: 0,
            theme_mode: "auto".into(),
            screen_mode: 0,
            panel_layout: 0,
            language: "ko".into(),
            is_metric: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RenderedFrame {
    pub width: i32,
    pub height: i32,
    pub rgba: Vec<u8>,
}
