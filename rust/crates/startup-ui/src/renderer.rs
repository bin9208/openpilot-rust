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
#[derive(Clone, Copy, Debug)]
pub struct TextureOptions {
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub premultiply: bool,
    pub keep_aspect: bool,
    pub flip_x: bool,
}
impl Default for TextureOptions {
    fn default() -> Self {
        Self {
            width: None,
            height: None,
            premultiply: false,
            keep_aspect: true,
            flip_x: false,
        }
    }
}
pub struct Window {
    pub config: Config,
    pub dimensions: (f32, f32),
    pub title: String,
    pub spinner: bool,
    pub language: String,
}
pub struct Renderer {
    pub(crate) surface: cxx::UniquePtr<ffi::Surface>,
    pub config: Config,
    pub(crate) dimensions: (f32, f32),
    pub(crate) polygon_shader: Option<u32>,
    normal: u32,
    medium: u32,
    pretendard: u32,
    display: Option<u32>,
    fallback: bool,
    fonts: HashMap<String, u32>,
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
        Self::new_window(
            Window {
                config,
                dimensions: (config.width(), config.height()),
                title: if spinner {
                    "Spinner".into()
                } else {
                    "Text Viewer".into()
                },
                spinner,
                language: language.into(),
            },
            assets,
        )
    }
    pub fn new_window(window: Window, assets: &Path) -> Result<Self, Error> {
        let Window {
            config,
            dimensions,
            title,
            spinner,
            language,
        } = window;
        if !dimensions.0.is_finite()
            || !dimensions.1.is_finite()
            || dimensions.0 <= 0.0
            || dimensions.1 <= 0.0
        {
            return Err(Error::Contract(
                "window dimensions must be finite and positive",
            ));
        }
        let scaled = |value: f32| {
            let value = crate::number::integer(value * config.scale)?;
            Ok::<_, Error>(value + value % 2)
        };
        let mut surface = ffi::create(
            scaled(dimensions.0)?,
            scaled(dimensions.1)?,
            &title,
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
                .render_target(scaled(dimensions.0)?, scaled(dimensions.1)?)?;
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
            dimensions,
            polygon_shader: None,
            normal,
            medium,
            pretendard,
            display,
            fallback: matches!(language.as_str(), "th" | "zh-CHT" | "zh-CHS" | "ko" | "ja"),
            fonts: fonts
                .into_iter()
                .map(|(name, id)| (name.to_owned(), id))
                .collect(),
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
}
mod assets;
mod fonts;
mod frame;
mod paint;
use fonts::{font_points, resolve_font};
use paint::convert;
