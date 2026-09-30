use crate::{
    bridge::ffi,
    config::Config,
    draw::{Draw, TextDraw},
    geometry::{Point, Rect},
    text::{Font, Measure},
    Error,
};
use std::{
    collections::HashMap,
    marker::PhantomData,
    path::{Path, PathBuf},
    rc::Rc,
};
pub struct Renderer {
    pub(crate) surface: cxx::UniquePtr<ffi::Surface>,
    pub config: Config,
    normal: u32,
    medium: u32,
    pretendard: u32,
    display: Option<u32>,
    fallback: bool,
    comma: Option<u32>,
    track: Option<u32>,
    _thread: PhantomData<Rc<()>>,
}
impl Renderer {
    pub fn new(
        config: Config,
        assets: &Path,
        spinner: bool,
        language: &str,
    ) -> Result<Self, Error> {
        let scaled = |value: f32| {
            let value = crate::number::integer(value * config.scale)?;
            Ok::<_, Error>(value + value % 2)
        };
        let mut surface = ffi::create(
            scaled(config.width())?,
            scaled(config.height())?,
            if spinner { "Spinner" } else { "Text Viewer" },
            32 | if std::env::var("ENABLE_VSYNC").as_deref() == Ok("1") {
                64
            } else {
                0
            },
        )?;
        surface
            .pin_mut()
            .target_fps(if std::env::var("OFFSCREEN").as_deref() == Ok("1") {
                0
            } else {
                20
            });
        if config.scale != 1.0 {
            surface
                .pin_mut()
                .render_target(scaled(config.width())?, scaled(config.height())?)?;
        }
        let normal_name = if config.big {
            "Inter-Regular"
        } else {
            "Inter-Medium"
        };
        let mut fonts = HashMap::new();
        let names = if spinner {
            vec![normal_name, "Pretendard-SemiBold"]
        } else {
            vec![
                normal_name,
                "Inter-Medium",
                "Inter-Bold",
                "Inter-SemiBold",
                "Pretendard-SemiBold",
                "unifont",
                "Inter-Regular",
                "KaiGenGothicKR-Bold",
            ]
        };
        for name in names {
            if fonts.contains_key(name) {
                continue;
            }
            let source = resolve_font(&assets.join("fonts"), name);
            let points = font_points(name);
            let id = surface.pin_mut().font(
                source
                    .to_str()
                    .ok_or(Error::Contract("font path is not UTF-8"))?,
                if name == "unifont" {
                    16
                } else if name == "KaiGenGothicKR-Bold" {
                    48
                } else {
                    200
                },
                &points,
                source.extension().is_some_and(|ext| ext == "fnt"),
                name != "unifont",
            )?;
            fonts.insert(name, id);
        }
        let normal = *fonts
            .get(normal_name)
            .ok_or(Error::Contract("normal font missing"))?;
        let medium = *fonts.get("Inter-Medium").unwrap_or(&normal);
        let pretendard = *fonts
            .get("Pretendard-SemiBold")
            .ok_or(Error::Contract("Pretendard font missing"))?;
        let display = fonts.get("KaiGenGothicKR-Bold").copied();
        let mut renderer = Self {
            surface,
            config,
            normal,
            medium,
            pretendard,
            display,
            fallback: matches!(language, "th" | "zh-CHT" | "zh-CHS" | "ko" | "ja"),
            comma: None,
            track: None,
            _thread: PhantomData,
        };
        if spinner {
            let size = crate::number::integer(config.spinner().texture)?;
            renderer.comma =
                Some(renderer.load_texture(&assets.join("img_spinner_comma.png"), size, false)?);
            renderer.track =
                Some(renderer.load_texture(&assets.join("img_spinner_track.png"), size, true)?);
        }
        Ok(renderer)
    }
    fn load_texture(&mut self, path: &Path, size: i32, premultiply: bool) -> Result<u32, Error> {
        let mut image = ffi::image(
            path.to_str()
                .ok_or(Error::Contract("texture path is not UTF-8"))?,
        )?;
        if premultiply {
            image.pin_mut().premultiply();
        }
        let width = if self.config.scale != 1.0 {
            crate::number::integer(crate::number::float(size) * self.config.scale)?
                .min(image.width())
        } else {
            size
        };
        let height = if self.config.scale != 1.0 {
            crate::number::integer(crate::number::float(size) * self.config.scale)?
                .min(image.height())
        } else {
            size
        };
        if image.width() != width || image.height() != height {
            let scale = (f64::from(width) / f64::from(image.width()))
                .min(f64::from(height) / f64::from(image.height()));
            use num_traits::ToPrimitive;
            let w = (f64::from(image.width()) * scale)
                .to_i32()
                .ok_or(Error::Contract("image width out of range"))?;
            let h = (f64::from(image.height()) * scale)
                .to_i32()
                .ok_or(Error::Contract("image height out of range"))?;
            image.pin_mut().resize(w, h);
        }
        Ok(self.surface.pin_mut().texture(
            image.pin_mut(),
            if self.config.scale != 1.0 { size } else { 0 },
            if self.config.scale != 1.0 { size } else { 0 },
        )?)
    }
    fn font_id(&self, font: Font) -> u32 {
        match font {
            Font::NormalRaw => self.normal,
            Font::Normal => {
                if self.fallback {
                    self.display.unwrap_or(self.normal)
                } else {
                    self.normal
                }
            }
            Font::Medium => {
                if self.fallback {
                    self.display.unwrap_or(self.medium)
                } else {
                    self.medium
                }
            }
            Font::Pretendard => self.pretendard,
            Font::Display => self.display.unwrap_or(self.normal),
        }
    }
    pub fn begin(&mut self) {
        self.surface.pin_mut().begin(self.config.scale);
    }
    pub fn end(&mut self) {
        self.surface.pin_mut().end(self.config.scale);
    }
    pub fn screenshot(&self, path: &Path) -> Result<(), Error> {
        Ok(self.surface.screenshot(
            path.to_str()
                .ok_or(Error::Contract("screenshot path is not UTF-8"))?,
        )?)
    }
    pub fn should_close(&self) -> bool {
        self.surface.should_close()
    }
    pub fn frame_time(&self) -> f32 {
        self.surface.frame_time()
    }
    pub fn time(&self) -> f64 {
        self.surface.time()
    }
    pub fn wheel(&self) -> f32 {
        self.surface.wheel()
    }
    pub fn sample(&self, slot: i32) -> (Point, bool) {
        let sample = self.surface.sample(slot);
        (
            Point {
                x: sample.x / self.config.scale,
                y: sample.y / self.config.scale,
            },
            sample.down,
        )
    }
}
impl Measure for Renderer {
    fn measure(&self, font: Font, text: &str, size: f32, spacing: f32) -> Point {
        let (plain, count) = if font == Font::Normal {
            crate::text::strip_emoji(text)
        } else {
            (text.to_owned(), 0)
        };
        match self
            .surface
            .measure(self.font_id(font), &plain, size, spacing)
        {
            Ok(value) => Point {
                x: value.x + crate::number::float(count) * size,
                y: if count > 0 && value.y == 0.0 {
                    size
                } else {
                    value.y
                },
            },
            Err(error) => {
                eprintln!("text measurement: {error}");
                Point::default()
            }
        }
    }
}
impl Draw for Renderer {
    fn text(&mut self, text: TextDraw<'_>) -> Result<(), Error> {
        let font = self.font_id(text.font);
        Ok(self.surface.pin_mut().text(
            font,
            text.text,
            ffi::Point {
                x: text.position.x,
                y: text.position.y,
            },
            text.size,
            text.spacing,
            text.color,
        )?)
    }
    fn rounded(&mut self, rect: Rect, roundness: f32, color: u32) -> Result<(), Error> {
        self.surface
            .pin_mut()
            .rounded(convert(rect), roundness, color, false);
        Ok(())
    }
    fn border(&mut self, rect: Rect, roundness: f32, color: u32) -> Result<(), Error> {
        self.surface
            .pin_mut()
            .rounded(convert(rect), roundness, color, true);
        Ok(())
    }
    fn texture(
        &mut self,
        track: bool,
        rect: Rect,
        origin: Point,
        rotation: f32,
    ) -> Result<(), Error> {
        let texture = if track { self.track } else { self.comma }
            .ok_or(Error::Contract("spinner texture not loaded"))?;
        Ok(self.surface.pin_mut().draw_texture(
            texture,
            convert(rect),
            ffi::Point {
                x: origin.x,
                y: origin.y,
            },
            rotation,
        )?)
    }
    fn scissor(&mut self, rect: Option<Rect>) -> Result<(), Error> {
        let rect = rect.map(|rect| Rect {
            x: (rect.x.trunc() * self.config.scale).trunc(),
            y: (rect.y.trunc() * self.config.scale).trunc(),
            width: (rect.width.trunc() * self.config.scale).ceil(),
            height: (rect.height.trunc() * self.config.scale).ceil(),
        });
        self.surface
            .pin_mut()
            .scissor(convert(rect.unwrap_or_default()), rect.is_some());
        Ok(())
    }
}
fn convert(rect: Rect) -> ffi::Rect {
    ffi::Rect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}
fn resolve_font(root: &Path, name: &str) -> PathBuf {
    let path = root.join(format!("{name}.fnt"));
    if path.exists() {
        return path;
    }
    for extension in ["ttf", "otf"] {
        let source = root.join(format!("{name}.{extension}"));
        if source.exists() {
            eprintln!(
                "Font atlas missing, loading source font instead: {}",
                source.display()
            );
            return source;
        }
    }
    path
}
fn font_points(name: &str) -> Vec<i32> {
    let mut points: Vec<_> = (32..127).collect();
    if matches!(name, "unifont" | "KaiGenGothicKR-Bold") {
        points.extend(0xAC00..0xD7A4);
        points.extend(0x4E00..0xA000);
        points.extend(0x3400..0x4DC0);
        points.sort_unstable();
    }
    points
}
