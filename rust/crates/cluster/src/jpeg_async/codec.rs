//! Source codec selection and timing policy; native resources perform only JPEG calls.
use super::{Frame, Samples};
use crate::runtime_args::JpegEncoder;
use openpilot_jpeg::{Color, Layout, Options, Quality, TurboJpeg};
use std::{io, path::PathBuf, time::Instant};
#[derive(Clone, Debug)]
pub struct Config {
    pub quality: i64,
    pub encoder: JpegEncoder,
    pub turbo_library: PathBuf,
    pub profile_enabled: bool,
}
impl Config {
    /// # Errors
    /// Returns a non-Unicode external provider override.
    pub fn from_environment(
        quality: i64,
        encoder: JpegEncoder,
        profile_enabled: bool,
    ) -> io::Result<Self> {
        let library = match std::env::var("CLUSTER_JPEG_TURBOJPEG_LIBRARY") {
            Ok(value) => value,
            Err(std::env::VarError::NotPresent) => "libturbojpeg.so.0".into(),
            Err(error) => return Err(io::Error::other(error)),
        };
        Ok(Self {
            quality,
            encoder,
            turbo_library: library.into(),
            profile_enabled,
        })
    }
}
pub struct Codec {
    config: Config,
    turbo: Option<TurboJpeg>,
    unavailable: bool,
    samples: Samples,
}
impl Codec {
    #[must_use]
    pub fn new(config: Config) -> Self {
        Self {
            config,
            turbo: None,
            unavailable: false,
            samples: Vec::new(),
        }
    }
    pub fn clear_profile_samples(&mut self) {
        self.samples.clear();
    }
    #[must_use]
    pub fn profile_samples(&self) -> &[(String, f64)] {
        &self.samples
    }
    fn sample(&mut self, name: &str, start: Instant) {
        if self.config.profile_enabled {
            self.samples
                .push((name.into(), start.elapsed().as_secs_f64() * 1000.0));
        }
    }
    /// # Errors
    /// Returns selected provider, packed pixel contract or JPEG compressor failure.
    pub fn encode(&mut self, frame: &Frame) -> io::Result<Vec<u8>> {
        self.encode_pixels(&frame.rgba, frame.width, frame.height)
    }
    /// Borrows packed RGBA synchronously; no pixel ownership escapes this call.
    /// # Errors
    /// Returns selected provider, packed pixel contract or JPEG compressor failure.
    pub fn encode_pixels(&mut self, rgba: &[u8], width: u32, height: u32) -> io::Result<Vec<u8>> {
        let turbo = match self.config.encoder {
            JpegEncoder::Auto => !self.unavailable,
            JpegEncoder::Turbojpeg => true,
            JpegEncoder::Pillow => false,
        };
        if turbo {
            match self.encode_turbo(rgba, width, height) {
                Ok(jpeg) => return Ok(jpeg),
                Err(error) => match self.config.encoder {
                    JpegEncoder::Turbojpeg => return Err(error),
                    JpegEncoder::Auto => self.unavailable = true,
                    JpegEncoder::Pillow => unreachable!("Pillow never selects TurboJPEG"),
                },
            }
        }
        self.encode_pillow(rgba, width, height)
    }
    fn options(&self) -> io::Result<Options> {
        Ok(Options::new(
            Quality::new(u8::try_from(self.config.quality.clamp(1, 95)).map_err(io::Error::other)?)
                .map_err(io::Error::other)?,
        ))
    }
    fn layout(rgba: &[u8], width: u32, height: u32) -> io::Result<Layout> {
        let layout = Layout::new(width, height, Color::Rgbx).map_err(io::Error::other)?;
        layout.check_pixels(rgba).map_err(io::Error::other)?;
        Ok(layout)
    }
    fn encode_turbo(&mut self, rgba: &[u8], width: u32, height: u32) -> io::Result<Vec<u8>> {
        let start = Instant::now();
        self.sample("usb.encode.turbojpeg_import", start);
        let start = Instant::now();
        let layout = Self::layout(rgba, width, height)?;
        self.sample("usb.encode.turbojpeg_rgba_view", start);
        let start = Instant::now();
        if self.turbo.is_none() {
            self.turbo =
                Some(TurboJpeg::open(&self.config.turbo_library).map_err(io::Error::other)?);
        }
        let options = self.options()?;
        let jpeg = self
            .turbo
            .as_mut()
            .ok_or_else(|| io::Error::other("TurboJPEG instance missing"))?
            .encode(rgba, layout, options)
            .map_err(io::Error::other)?;
        self.sample("usb.encode.turbojpeg_encode", start);
        Ok(jpeg)
    }
    fn encode_pillow(&mut self, rgba: &[u8], width: u32, height: u32) -> io::Result<Vec<u8>> {
        let start = Instant::now();
        let layout = Self::layout(rgba, width, height)?;
        self.sample("usb.encode.rgba_to_rgbx_view", start);
        let start = Instant::now();
        let jpeg =
            openpilot_jpeg::encode_with(rgba, layout, self.options()?).map_err(io::Error::other)?;
        self.sample("usb.encode.jpeg_save", start);
        let start = Instant::now();
        self.sample("usb.encode.getvalue", start);
        Ok(jpeg)
    }
}
