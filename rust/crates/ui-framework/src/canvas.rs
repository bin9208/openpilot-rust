pub use crate::assets::Texture;
use crate::{
    draw::{Draw, ImageDraw, TextDraw},
    geometry::{Point, Rect},
    text::{Font, Measure},
    Error,
};
use openpilot_startup_ui::renderer::{Renderer, TextureOptions};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
pub struct Canvas {
    pub renderer: Renderer,
    root: PathBuf,
    emoji_font: Option<Vec<u8>>,
    emojis: HashMap<String, u32>,
    textures: HashMap<String, Texture>,
}
impl Canvas {
    pub fn new(renderer: Renderer, assets: &Path) -> Self {
        Self {
            renderer,
            root: assets.to_owned(),
            emoji_font: None,
            emojis: HashMap::new(),
            textures: HashMap::new(),
        }
    }
    pub fn texture(&mut self, path: &str, options: TextureOptions) -> Result<Texture, Error> {
        let key = format!("{path}:{options:?}");
        if let Some(texture) = self.textures.get(&key) {
            return Ok(*texture);
        }
        let (id, width, height) = self.renderer.load_asset(&self.root.join(path), options)?;
        use num_traits::ToPrimitive;
        let texture = Texture {
            id,
            width: width
                .to_f32()
                .ok_or(Error::Contract("texture width overflow"))?,
            height: height
                .to_f32()
                .ok_or(Error::Contract("texture height overflow"))?,
        };
        self.textures.insert(key, texture);
        Ok(texture)
    }
}
impl Measure for Canvas {
    fn measure_default(&self, text: &str, size: i32) -> Result<i32, Error> {
        self.renderer.measure_default(text, size)
    }

    fn measure(&self, font: Font, text: &str, size: f32, spacing: f32) -> Point {
        self.renderer
            .measure_raw(font, text, size, spacing)
            .unwrap_or_else(|error| {
                eprintln!("UI text measurement: {error}");
                Point::default()
            })
    }
}
impl Draw for Canvas {
    fn spline(&mut self, points: &[Point], thick: f32, color: u32) -> Result<(), Error> {
        self.renderer.spline(points, thick, color)
    }
    fn camera_plane(
        &mut self,
        dimensions: (i32, i32),
        format: openpilot_startup_ui::camera::PlaneFormat,
    ) -> Result<Box<dyn openpilot_startup_ui::draw::TextureResource>, Error> {
        Ok(Box::new(self.renderer.camera_plane(dimensions, format)?))
    }
    fn update_camera_plane(&mut self, texture: u32, bytes: &[u8]) -> Result<(), Error> {
        self.renderer.update_camera_plane(texture, bytes)
    }
    fn native_texture(&self, texture: u32) -> Result<u32, Error> {
        self.renderer.native_texture(texture)
    }
    fn camera(&mut self, camera: openpilot_startup_ui::camera::CameraDraw) -> Result<(), Error> {
        self.renderer.camera(camera)
    }
    fn ring(&mut self, ring: crate::draw::Ring) -> Result<(), Error> {
        self.renderer.ring(ring);
        Ok(())
    }
    fn upload_image(
        &mut self,
        pixels: crate::draw::PixelBuffer<'_>,
    ) -> Result<Box<dyn crate::draw::TextureResource>, Error> {
        Ok(Box::new(self.renderer.dynamic_image(pixels)?))
    }

    fn rounded_outline(
        &mut self,
        rect: Rect,
        style: crate::draw::RoundedOutline,
    ) -> Result<(), Error> {
        self.renderer.rounded_outline(rect, style);
        Ok(())
    }

    fn clear(&mut self, color: u32) -> Result<(), Error> {
        self.renderer.clear(color)
    }
    fn upload_pixels(
        &mut self,
        pixels: crate::draw::PixelBuffer<'_>,
    ) -> Result<Box<dyn crate::draw::TextureResource>, Error> {
        Ok(Box::new(self.renderer.dynamic_pixels(
            pixels.dimensions.0,
            pixels.dimensions.1,
            pixels.rgba,
        )?))
    }

    fn rectangle_lines(&mut self, rect: Rect, color: u32) -> Result<(), Error> {
        self.renderer.rectangle_lines(rect, color)
    }

    fn triangle_strip(&mut self, points: &[Point], color: u32) -> Result<(), Error> {
        self.renderer.triangle_strip(points, color)
    }
    fn shaded_strip(
        &mut self,
        points: &[Point],
        paint: crate::draw::PolygonPaint<'_>,
    ) -> Result<(), Error> {
        self.renderer.shaded_strip(points, paint)
    }

    fn rounded_segments(
        &mut self,
        rect: Rect,
        roundness: f32,
        segments: i32,
        color: u32,
        border: bool,
    ) -> Result<(), Error> {
        self.renderer
            .rounded_segments(rect, roundness, segments, color, border);
        Ok(())
    }

    fn font_scale(&self) -> f64 {
        if self.renderer.config.big {
            1.242
        } else {
            1.16
        }
    }
    fn text(&mut self, text: TextDraw<'_>) -> Result<(), Error> {
        self.renderer.text(text)
    }
    fn rounded(&mut self, rect: Rect, roundness: f32, color: u32) -> Result<(), Error> {
        self.renderer.rounded(rect, roundness, color)
    }
    fn border(&mut self, rect: Rect, roundness: f32, color: u32) -> Result<(), Error> {
        self.renderer.border(rect, roundness, color)
    }
    fn texture(
        &mut self,
        track: bool,
        rect: Rect,
        origin: Point,
        rotation: f32,
    ) -> Result<(), Error> {
        self.renderer.texture(track, rect, origin, rotation)
    }
    fn scissor(&mut self, rect: Option<Rect>) -> Result<(), Error> {
        self.renderer.scissor(rect)
    }
    fn circle(&mut self, center: Point, radius: f32, color: u32) -> Result<(), Error> {
        self.renderer.circle(center, radius, color);
        Ok(())
    }
    fn circle_gradient(
        &mut self,
        center: Point,
        radius: f32,
        colors: [u32; 2],
    ) -> Result<(), Error> {
        self.renderer.circle_gradient(center, radius, colors);
        Ok(())
    }
    fn gradient(&mut self, rect: Rect, colors: [u32; 4]) -> Result<(), Error> {
        self.renderer.gradient(rect, colors);
        Ok(())
    }
    fn line(&mut self, start: Point, end: Point, thick: f32, color: u32) -> Result<(), Error> {
        self.renderer.line(start, end, thick, color);
        Ok(())
    }
    fn image(&mut self, image: ImageDraw) -> Result<(), Error> {
        self.renderer.tinted_texture(
            image.id,
            image.source,
            image.destination,
            image.origin,
            image.rotation,
            image.tint,
        )
    }
    fn emoji(&mut self, text: &str, position: Point, size: f32, tint: u32) -> Result<(), Error> {
        let id = if let Some(id) = self.emojis.get(text) {
            *id
        } else {
            if self.emoji_font.is_none() {
                self.emoji_font = Some(std::fs::read(self.root.join("fonts/NotoColorEmoji.ttf"))?);
            }
            let pixels = crate::emoji::rasterize(
                self.emoji_font
                    .as_deref()
                    .ok_or(Error::Contract("emoji font missing"))?,
                text,
            )?;
            let id = self.renderer.pixel_texture(128, 128, &pixels)?;
            self.emojis.insert(text.to_owned(), id);
            id
        };
        self.image(ImageDraw {
            id,
            source: Rect {
                x: 0.0,
                y: 0.0,
                width: 128.0,
                height: 128.0,
            },
            destination: Rect {
                x: position.x,
                y: position.y,
                width: size,
                height: size,
            },
            origin: Point::default(),
            rotation: 0.0,
            tint,
        })
    }
}
