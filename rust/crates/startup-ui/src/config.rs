#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub big: bool,
    pub large_viewport: bool,
    pub pc: bool,
    pub scale: f32,
}
impl Config {
    #[cfg(feature = "native")]
    pub fn for_runtime() -> Result<(Self, String), crate::Error> {
        use crate::Error;
        let hardware = openpilot_hardware_info::for_runtime();
        let device = hardware
            .get_device_type()
            .map_err(|_| Error::Contract("hardware type failed"))?;
        let pc = device == "pc";
        let big = std::env::var("BIG").as_deref() == Ok("1");
        let large_viewport = big || matches!(device.as_str(), "tici" | "tizi");
        let mut config = Config {
            big,
            large_viewport,
            pc,
            scale: std::env::var("SCALE")
                .unwrap_or_else(|_| "1.0".into())
                .parse()
                .map_err(|_| Error::Contract("invalid SCALE"))?,
        };
        if !config.scale.is_finite() || config.scale <= 0.0 {
            return Err(Error::Contract("SCALE must be finite and positive"));
        }
        if pc && std::env::var_os("SCALE").is_none() {
            let monitor = crate::bridge::ffi::monitor()?;
            if monitor.x > 0.0
                && monitor.y > 0.0
                && (monitor.x < config.width() || monitor.y < config.height())
            {
                config.scale = (monitor.x / config.width())
                    .min(monitor.y / config.height())
                    .mul_add(0.95, 0.0)
                    .max(0.3);
            }
        }
        Ok((config, device))
    }

    pub fn width(self) -> f32 {
        if self.large_viewport {
            2160.0
        } else {
            536.0
        }
    }
    pub fn height(self) -> f32 {
        if self.large_viewport {
            1080.0
        } else {
            240.0
        }
    }
    pub fn scaled_font(self, size: f32) -> f32 {
        crate::number::float(f64::from(size) * if self.big { 1.242 } else { 1.16 })
    }
    pub fn spinner(self) -> SpinnerTokens {
        if self.large_viewport {
            SpinnerTokens {
                texture: 360.0,
                bar_width: 1000.0,
                bar_height: 20.0,
                wrapped_gap: 50.0,
                center_gap: 150.0,
                margin: 100.0,
                font: 96.0,
                line_height: 104.0,
                ip_font: 72.0,
                ip_top: 36.0,
                status_font: 48.0,
                status_gap: 14.0,
            }
        } else {
            SpinnerTokens {
                texture: 140.0,
                bar_width: 268.0,
                bar_height: 10.0,
                wrapped_gap: 10.0,
                center_gap: 20.0,
                margin: 20.0,
                font: 28.0,
                line_height: 32.0,
                ip_font: 36.0,
                ip_top: 12.0,
                status_font: 28.0,
                status_gap: 4.0,
            }
        }
    }
    pub fn text(self) -> TextTokens {
        let (margin, gap, font, line_height, button_width, button_height) = if self.big {
            (50.0, 40.0, 72.0, 80.0, 310.0, 160.0)
        } else {
            (20.0, 30.0, 25.0, 25.0, 150.0, 80.0)
        };
        TextTokens {
            margin,
            gap,
            font,
            line_height,
            button_width,
            button_height,
            ip_font: if self.large_viewport { 50.0 } else { 18.0 },
            ip_band: if self.large_viewport { 80.0 } else { 28.0 },
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct SpinnerTokens {
    pub texture: f32,
    pub bar_width: f32,
    pub bar_height: f32,
    pub wrapped_gap: f32,
    pub center_gap: f32,
    pub margin: f32,
    pub font: f32,
    pub line_height: f32,
    pub ip_font: f32,
    pub ip_top: f32,
    pub status_font: f32,
    pub status_gap: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct TextTokens {
    pub margin: f32,
    pub gap: f32,
    pub font: f32,
    pub line_height: f32,
    pub button_width: f32,
    pub button_height: f32,
    pub ip_font: f32,
    pub ip_band: f32,
}
