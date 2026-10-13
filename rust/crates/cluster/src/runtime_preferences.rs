use crate::{
    config,
    renderer::display::normalize_language,
    runtime_args::{Codec, H264Backend, RuntimeArgs},
};
use openpilot_params::Params;
use openpilot_ui_application::params::{numeric, typed};

#[derive(Clone, Debug, serde::Serialize)]
pub struct DisplayPreferences {
    pub language: &'static str,
    pub is_metric: bool,
}
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct RadarPreferences {
    pub info: i64,
    pub display: i64,
    pub color: i64,
}
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct OutputGate {
    pub mode: i64,
    pub allowed: bool,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Snapshot {
    pub theme: &'static str,
    pub display: DisplayPreferences,
    pub clock_visible: bool,
    pub usb_fps: f64,
    pub brightness: i64,
    pub orientation: i64,
    pub mirror: i64,
    pub screen_mode: i64,
    pub camera_view_mode: i64,
    pub panel_layout: i64,
    pub radar: RadarPreferences,
    pub hud_mode: Option<i64>,
    pub encoder: Option<i64>,
    pub gate: OutputGate,
}
pub struct Reader {
    params: Option<Params>,
}
impl Reader {
    #[must_use]
    pub fn new(params: Option<Params>) -> Self {
        Self { params }
    }
    #[must_use]
    pub fn for_runtime() -> Self {
        Self::new(Params::for_runtime().ok())
    }
    fn bytes(&self, key: &str) -> Option<Vec<u8>> {
        self.params.as_ref()?.get(key).ok().flatten()
    }
    fn integer(&self, key: &str) -> Option<i64> {
        self.params.as_ref()?;
        let bytes = self.bytes(key).unwrap_or_default();
        let value = numeric::integer(&bytes).unwrap_or_else(|| {
            eprintln!("[cluster] fatal malformed integer Params value: {key}");
            std::process::abort()
        });
        Some(i64::from(value))
    }
    fn default_bytes(&self, key: &str) -> Vec<u8> {
        self.bytes(key)
            .filter(|bytes| !bytes.is_empty())
            .unwrap_or_else(|| {
                openpilot_params::metadata(key)
                    .and_then(|item| item.default)
                    .unwrap_or_default()
                    .as_bytes()
                    .to_vec()
            })
    }
    fn display(&self) -> DisplayPreferences {
        let fallback = DisplayPreferences {
            language: "ko",
            is_metric: true,
        };
        if self.params.is_none() {
            return fallback;
        }
        let Ok(language) = String::from_utf8(self.default_bytes("LanguageSetting")) else {
            return fallback;
        };
        DisplayPreferences {
            language: normalize_language(&language, "ko"),
            is_metric: self.default_bytes("IsMetric") == b"1",
        }
    }
    fn radar(&self, key: &str, default: i64, maximum: i64) -> i64 {
        self.bytes(key)
            .as_deref()
            .and_then(typed::integer_bytes)
            .and_then(|value| value.parse().ok())
            .filter(|value| (0..=maximum).contains(value))
            .unwrap_or(default)
    }
    #[must_use]
    pub fn encoder(&self) -> Option<i64> {
        self.integer(config::CLUSTER_ENCODER_PARAM).map(|value| {
            if (0..=3).contains(&value) {
                value
            } else {
                0
            }
        })
    }
    #[must_use]
    pub fn usb_fps(&self) -> f64 {
        if self.bytes("UsbGpuActive").as_deref() == Some(b"1") {
            5.0
        } else {
            10.0
        }
    }
    #[must_use]
    pub fn gate(&self) -> OutputGate {
        let mode = self
            .integer(config::CLUSTER_HUD_DEBUG_PARAM)
            .unwrap_or(0)
            .clamp(0, 3);
        OutputGate {
            mode,
            allowed: self.params.is_none()
                || mode >= 1
                || self.bytes("IsOnroad").as_deref() == Some(b"1"),
        }
    }
    #[must_use]
    pub fn brightness(&self) -> i64 {
        self.integer(config::CLUSTER_BRIGHTNESS_PARAM)
            .unwrap_or(0)
            .clamp(0, 100)
    }
    #[must_use]
    pub fn orientation(&self) -> i64 {
        let value = self.integer(config::CLUSTER_ORIENTATION_PARAM).unwrap_or(0);
        if (0..=3).contains(&value) {
            value
        } else {
            0
        }
    }
    #[must_use]
    pub fn mirror(&self) -> i64 {
        self.integer(config::CLUSTER_HUD_MIRROR_PARAM)
            .unwrap_or(0)
            .clamp(0, 3)
    }
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        let orientation = self.integer(config::CLUSTER_ORIENTATION_PARAM).unwrap_or(0);
        let screen = self.integer(config::CLUSTER_SCREEN_MODE_PARAM).unwrap_or(0);
        let camera = self
            .integer(config::CLUSTER_CAMERA_VIEW_MODE_PARAM)
            .unwrap_or(0);
        Snapshot {
            theme: match self.integer(config::CLUSTER_THEME_PARAM) {
                Some(1) => "dark",
                Some(2) => "light",
                _ => "auto",
            },
            display: self.display(),
            clock_visible: self.params.is_none()
                || matches!(self.integer("ShowDateTime"), Some(1 | 2)),
            usb_fps: self.usb_fps(),
            brightness: self
                .integer(config::CLUSTER_BRIGHTNESS_PARAM)
                .unwrap_or(0)
                .clamp(0, 100),
            orientation: if (0..=3).contains(&orientation) {
                orientation
            } else {
                0
            },
            mirror: self
                .integer(config::CLUSTER_HUD_MIRROR_PARAM)
                .unwrap_or(0)
                .clamp(0, 3),
            screen_mode: if (-1..=6).contains(&screen) {
                screen
            } else {
                0
            },
            camera_view_mode: if (0..=4).contains(&camera) { camera } else { 0 },
            panel_layout: i64::from(self.integer(config::CLUSTER_PANEL_LAYOUT_PARAM) == Some(1)),
            radar: RadarPreferences {
                info: self.radar(config::CLUSTER_RADAR_INFO_PARAM, 4, 4),
                display: self.radar(config::CLUSTER_RADAR_DISPLAY_PARAM, 0, 1),
                color: self.radar(config::CLUSTER_RADAR_SOURCE_COLOR_PARAM, 0, 1),
            },
            hud_mode: self.integer(config::CLUSTER_HUD_PARAM),
            encoder: self.encoder(),
            gate: self.gate(),
        }
    }
    pub fn apply_encoder(&self, args: &mut RuntimeArgs) -> &'static str {
        if !args.usb_output() {
            return "disabled";
        }
        if args.codec_from_cli() {
            return "--usb-codec";
        }
        let Some(encoder) = self.encoder() else {
            return "default";
        };
        if encoder == 1 {
            args.usb_codec = Codec::Jpeg;
        } else {
            args.usb_codec = Codec::H264;
            if !args.encoder_presence.usb_h264_backend_from_cli {
                args.usb_h264_backend = if encoder == 3 {
                    H264Backend::Ffmpeg
                } else {
                    H264Backend::Native
                };
            }
            if args.usb_h264_backend == H264Backend::Ffmpeg
                && !args.encoder_presence.usb_h264_ffmpeg_encoder_from_cli
            {
                args.usb_h264_ffmpeg_encoder = "libx264".into();
            }
        }
        config::CLUSTER_ENCODER_PARAM
    }
}
