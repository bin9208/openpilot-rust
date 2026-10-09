//! `cluster_autorun.py` encoder order, live arguments and HUD gates.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Encoder {
    Auto,
    Jpeg,
    Hardware,
    Software,
}
impl Encoder {
    #[must_use]
    pub const fn from_setting(value: i64) -> Self {
        match value {
            1 => Self::Jpeg,
            2 => Self::Hardware,
            3 => Self::Software,
            _ => Self::Auto,
        }
    }
    #[must_use]
    pub const fn setting(self) -> i64 {
        match self {
            Self::Auto => 0,
            Self::Jpeg => 1,
            Self::Hardware => 2,
            Self::Software => 3,
        }
    }
    #[must_use]
    pub fn sequence(self, board: bool) -> &'static [Self] {
        match self {
            Self::Auto if board => &[Self::Hardware, Self::Software, Self::Jpeg],
            Self::Auto => &[Self::Software, Self::Jpeg],
            Self::Jpeg => &[Self::Jpeg],
            Self::Hardware => &[Self::Hardware],
            Self::Software => &[Self::Software],
        }
    }
    fn args(self) -> &'static [&'static str] {
        match self {
            Self::Auto | Self::Jpeg => &["--usb-codec", "jpeg", "--usb-jpeg-quality", "68"],
            Self::Hardware => &["--usb-codec", "h264", "--usb-h264-backend", "native"],
            Self::Software => &[
                "--usb-codec",
                "h264",
                "--usb-h264-backend",
                "ffmpeg",
                "--usb-h264-ffmpeg-encoder",
                "libx264",
            ],
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Output {
    Window,
    Usb,
    Both,
}
impl Output {
    #[must_use]
    pub const fn usb(self) -> bool {
        match self {
            Self::Window => false,
            Self::Usb | Self::Both => true,
        }
    }
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Window => "window",
            Self::Usb => "usb",
            Self::Both => "both",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct RunRequest {
    pub hud_mode: i64,
    pub configured_encoder: Encoder,
    pub active_encoder: Encoder,
    pub output: Output,
    pub usbgpu_active: bool,
}
impl RunRequest {
    #[must_use]
    pub fn args(self) -> Vec<String> {
        let mut args = vec![
            "--input".into(),
            "live".into(),
            "--output".into(),
            self.output.name().into(),
        ];
        if self.output.usb() {
            args.extend(
                self.active_encoder
                    .args()
                    .iter()
                    .map(|value| (*value).to_owned()),
            );
        }
        args.extend([
            "--cluster-hud-mode".into(),
            self.hud_mode.to_string(),
            "--cluster-hud-encoder".into(),
            self.configured_encoder.setting().to_string(),
            "--fps".into(),
            fixed_fps(self.usbgpu_active).to_string(),
        ]);
        if self.output.usb() {
            args.extend([
                "--usb-display-fps".into(),
                fixed_fps(self.usbgpu_active).to_string(),
            ]);
        }
        args
    }
}

#[must_use]
pub const fn fixed_fps(usbgpu_active: bool) -> u8 {
    if usbgpu_active {
        5
    } else {
        10
    }
}
#[must_use]
pub const fn product_id(hud_mode: i64) -> Option<u16> {
    if hud_mode == 1 {
        Some(0x0092)
    } else {
        None
    }
}
#[must_use]
pub const fn output_allowed(debug: i64, onroad: bool) -> bool {
    debug >= 1 || onroad
}
#[must_use]
pub const fn orientation(setting: i64) -> Option<u8> {
    match setting {
        0 => Some(0),
        2 => Some(2),
        _ => None,
    }
}
