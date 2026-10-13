mod choices;
mod flags;
mod parser;
mod validate;
mod value;
pub use choices::*;
pub use flags::*;
use serde::Serialize;
use std::path::{Path, PathBuf};
use value::ArgumentValue;

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Help(String),
}

macro_rules! arguments {
    ($($field:ident : $kind:ty = $default:expr => $option:literal),+ $(,)?) => {
        #[derive(Clone, Debug, Serialize)]
        pub struct RuntimeArgs {
            $(pub $field: $kind,)+
            pub is_metric: Option<bool>,
            #[serde(flatten)] pub navi: NaviFlags,
            #[serde(flatten)] pub transport: TransportFlags,
            #[serde(flatten)] pub ack: AckFlags,
            #[serde(flatten)] pub patterns: PatternFlags,
            #[serde(flatten)] pub replay: ReplayFlags,
            #[serde(flatten)] pub live: LiveFlags,
            #[serde(flatten)] pub diagnostics: DiagnosticFlags,
            #[serde(flatten)] pub runtime: RuntimeFlags,
            #[serde(flatten)] pub rate_presence: RatePresence,
            #[serde(flatten)] pub encoder_presence: EncoderPresence,
        }
        impl Default for RuntimeArgs {
            fn default() -> Self {
                Self { $($field: $default,)+ is_metric: None, navi: NaviFlags::default(),
                    transport: TransportFlags::default(), ack: AckFlags::default(),
                    patterns: PatternFlags::default(), replay: ReplayFlags::default(),
                    live: LiveFlags::default(), diagnostics: DiagnosticFlags::default(),
                    runtime: RuntimeFlags::default(), rate_presence: RatePresence::default(),
                    encoder_presence: EncoderPresence::default() }
            }
        }
        const VALUE_OPTIONS: &[&str] = &[$($option),+];
        impl RuntimeArgs {
            fn apply_value(&mut self, option: &str, value: &str) -> Result<(), ParseError> {
                match option {
                    $($option => self.$field = <$kind>::parse_argument(value)
                        .map_err(|error| ParseError::Invalid(format!("argument {option}: {error}")))?,)+
                    _ => unreachable!("resolved value option"),
                }
                Ok(())
            }
        }
    };
}
arguments! {
    fps: f64 = 0.0 => "--fps",
    duration: Option<f64> = None => "--duration",
    input: InputSource = InputSource::Random => "--input",
    navi_host: String = "0.0.0.0".into() => "--navi-host",
    navi_port: i64 = 7714 => "--navi-port",
    navi_advertise_ip: Option<String> = None => "--navi-advertise-ip",
    navi_map_theme: Theme = Theme::Dark => "--navi-map-theme",
    output: Output = Output::Usb => "--output",
    controller_index: i64 = 0 => "--controller-index",
    width: Option<i64> = None => "--width",
    height: Option<i64> = None => "--height",
    usb_brightness: Option<i64> = None => "--usb-brightness",
    usb_display_fps: Option<i64> = None => "--usb-display-fps",
    usb_codec: Codec = Codec::Jpeg => "--usb-codec",
    usb_jpeg_quality: i64 = 68 => "--usb-jpeg-quality",
    usb_jpeg_encoder: JpegEncoder = JpegEncoder::Auto => "--usb-jpeg-encoder",
    usb_h264_bitrate: String = "auto".into() => "--usb-h264-bitrate",
    usb_h264_fps: i64 = 30 => "--usb-h264-fps",
    usb_h264_gop: i64 = 1 => "--usb-h264-gop",
    usb_h264_backend: H264Backend = H264Backend::Native => "--usb-h264-backend",
    usb_h264_library: String = String::new() => "--usb-h264-library",
    usb_h264_ffmpeg: String = "ffmpeg".into() => "--usb-h264-ffmpeg",
    usb_h264_ffmpeg_encoder: String = "libx264".into() => "--usb-h264-ffmpeg-encoder",
    usb_h264_device: String = "/dev/v4l/by-path/platform-aa00000.qcom_vidc-video-index1".into() => "--usb-h264-device",
    usb_h264_input_format: InputFormat = InputFormat::Nv12 => "--usb-h264-input-format",
    usb_h264_slice_max_bytes: i64 = 4096 => "--usb-h264-slice-max-bytes",
    usb_h264_rate_control: RateControl = RateControl::VbrCfr => "--usb-h264-rate-control",
    usb_h264_orientation: Orientation = Orientation::Portrait => "--usb-h264-orientation",
    usb_h264_align: i64 = 1 => "--usb-h264-align",
    usb_h264_encoder_align: i64 = 16 => "--usb-h264-encoder-align",
    usb_h264_chunk_size: i64 = 0 => "--usb-h264-chunk-size",
    usb_h264_dump: String = String::new() => "--usb-h264-dump",
    usb_h264_diagnose_interval: f64 = 0.0 => "--usb-h264-diagnose-interval",
    usb_frame_drain_attempts: i64 = 2 => "--usb-frame-drain-attempts",
    usb_frame_drain_timeout_ms: i64 = 2 => "--usb-frame-drain-timeout-ms",
    usb_fast_drain_attempts: i64 = 3 => "--usb-fast-drain-attempts",
    usb_fast_drain_timeout_ms: i64 = 2 => "--usb-fast-drain-timeout-ms",
    route: PathBuf = PathBuf::from("route") => "--route",
    route_log: RouteLog = RouteLog::Rlog => "--route-log",
    route_corner_source: CornerSource = CornerSource::Live => "--route-corner-source",
    route_overlay: RouteOverlay = RouteOverlay::Compact => "--route-overlay",
    route_tools: RouteTools = RouteTools::Overlay => "--route-tools",
    camera_view_mode: Option<i64> = None => "--camera-view-mode",
    panel_layout: Option<String> = None => "--panel-layout",
    theme: Option<Theme> = None => "--theme",
    language: Option<Language> = None => "--language",
    screen_mode: Option<String> = None => "--screen-mode",
    cluster_hud_mode: Option<i64> = None => "--cluster-hud-mode",
    cluster_hud_encoder: Option<i64> = None => "--cluster-hud-encoder",
    route_replay_speed: f64 = 1.0 => "--route-replay-speed",
    route_start_time: f64 = 0.0 => "--route-start-time",
    route_start_segment: Option<i64> = None => "--route-start-segment",
    route_max_segments: Option<i64> = None => "--route-max-segments",
    live_timeout_ms: i64 = 0 => "--live-timeout-ms",
    profile_interval: f64 = 2.0 => "--profile-interval",
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct OutputRate {
    pub target_fps: f64,
    pub fps_source: &'static str,
    pub fixed_usb_policy: bool,
    pub usb_display_fps: i64,
    pub usb_display_fps_auto: bool,
}
impl RuntimeArgs {
    /// Parse argv without the executable name. Root is the openpilot module directory.
    ///
    /// # Errors
    /// Returns usage requests, invalid options, or source policy validation failures.
    pub fn parse_from<S: AsRef<str>>(root: &Path, argv: &[S]) -> Result<Self, ParseError> {
        parser::parse(root, argv)
    }
    #[must_use]
    pub const fn fps_from_cli(&self) -> bool {
        self.rate_presence.fps_from_cli
    }
    #[must_use]
    pub const fn codec_from_cli(&self) -> bool {
        self.encoder_presence.usb_codec_from_cli
    }
    #[must_use]
    pub const fn usb_output(&self) -> bool {
        matches!(self.output, Output::Usb | Output::Both)
    }

    /// Resolve the rate using the current `UsbGpuActive` state.
    ///
    /// # Errors
    /// Fails when the source's rounded display rate cannot become an integer.
    pub fn output_rate(&self, tici: bool, egpu_active: bool) -> Result<OutputRate, crate::Error> {
        let fixed = tici && self.input == InputSource::Live && self.usb_output();
        let (target, source) = if fixed {
            (if egpu_active { 5.0 } else { 10.0 }, "fixed USB policy")
        } else if self.usb_output() && self.usb_codec == Codec::H264 && !self.fps_from_cli() {
            (
                num_traits::ToPrimitive::to_f64(&self.usb_h264_fps)
                    .ok_or(crate::Error::Contract("H264 FPS conversion failed"))?,
                "--usb-h264-fps",
            )
        } else {
            (
                self.fps,
                if self.fps_from_cli() {
                    "--fps"
                } else {
                    "default"
                },
            )
        };
        let display = if fixed {
            if egpu_active {
                5
            } else {
                10
            }
        } else if self.usb_output() {
            crate::rate::display_fps(
                self.usb_display_fps,
                self.usb_codec == Codec::H264,
                target,
                self.usb_h264_fps,
            )?
        } else {
            0
        };
        Ok(OutputRate {
            target_fps: target,
            fps_source: source,
            fixed_usb_policy: fixed,
            usb_display_fps: display,
            usb_display_fps_auto: fixed
                || (self.usb_output()
                    && self.usb_display_fps.is_none()
                    && self.usb_codec == Codec::H264),
        })
    }
}
