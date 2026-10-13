use super::{
    graphics::{color, point, scalar},
    tokens::{
        STROKED_TEXT_TEXTURE_CACHE_LIMIT, STROKED_TEXT_TEXTURE_PADDING_PX, TEXT_MEASURE_CACHE_LIMIT,
    },
    Anchor, ClusterUiRenderer, IntoColor, Point, Rect, Result, StrokeStyle,
};
use num_traits::ToPrimitive;
use openpilot_startup_ui::{
    draw::PixelBuffer,
    renderer::{DynamicTexture, RawImage, RawPoint, RawTextDraw, RawTextPaint, TextureFilter},
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct MeasureKey {
    font: u32,
    text: String,
    size: u64,
    spacing: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct StrokeKey {
    measure: MeasureKey,
    fill: [u8; 4],
    stroke: [u8; 4],
    width: i32,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct WorldKey {
    measure: MeasureKey,
    fill: [u8; 4],
}
pub(super) struct CachedText {
    texture: DynamicTexture,
    measured: (f64, f64),
    padding: f64,
}
pub(super) struct TextCachePolicy {
    pub world_labels: bool,
    pub strokes: bool,
    pub raw_strokes: bool,
}
impl TextCachePolicy {
    pub fn from_environment() -> Self {
        Self {
            world_labels: std::env::var("CLUSTER_WORLD_LABEL_TEXTURE_CACHE").as_deref() == Ok("1"),
            strokes: std::env::var("CLUSTER_STROKED_TEXT_TEXTURE_CACHE").as_deref() != Ok("0"),
            raw_strokes: std::env::var("CLUSTER_RAW_STROKED_TEXT").as_deref() != Ok("0"),
        }
    }
}

impl ClusterUiRenderer {
    fn font_for_text(&self, text: &str) -> u32 {
        if text.is_ascii() {
            self.font
        } else {
            self.korean_font.unwrap_or(self.font)
        }
    }
    /// # Errors
    /// Returns font, coordinate, or native drawing errors.
    pub fn measure_text(
        &mut self,
        text: &str,
        size: f64,
        spacing: Option<f64>,
    ) -> Result<(f64, f64)> {
        let spacing = spacing.unwrap_or_else(|| (size * 0.02).max(1.0));
        let key = MeasureKey {
            font: self.font_for_text(text),
            text: text.into(),
            size: size.to_bits(),
            spacing: spacing.to_bits(),
        };
        if let Some(value) = self.measure_cache.get(&key) {
            return Ok(*value);
        }
        if self.measure_cache.len() >= TEXT_MEASURE_CACHE_LIMIT {
            self.measure_cache.clear();
        }
        let value = self.gfx.backend()?.raw_font_measure(
            key.font,
            text,
            scalar(size)?,
            scalar(spacing)?,
        )?;
        let measured = (f64::from(value.x), f64::from(value.y));
        self.measure_cache.insert(key, measured);
        Ok(measured)
    }
    /// # Errors
    /// Returns font, coordinate, or native drawing errors.
    pub fn draw_text(
        &mut self,
        text: &str,
        x: f64,
        y: f64,
        size: f64,
        tint: impl IntoColor,
        anchor: Anchor,
    ) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        let measured = self.measure_text(text, size, None)?;
        let position = anchored(Point { x, y }, measured, anchor);
        let font = self.font_for_text(text);
        Ok(self.gfx.backend()?.raw_text(RawTextDraw {
            font,
            text,
            position: point(position)?,
            size: scalar(size)?,
            spacing: scalar((size * 0.02).max(1.0))?,
            color: color(tint),
        })?)
    }
    /// # Errors
    /// Returns font, coordinate, or native drawing errors.
    pub fn draw_text_with_stroke(
        &mut self,
        text: &str,
        x: f64,
        y: f64,
        size: f64,
        tint: impl IntoColor,
        style: StrokeStyle,
    ) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        let fill = tint.rgba();
        if style.cache && style.width > 0 && self.text_policy.strokes {
            let key = StrokeKey {
                measure: MeasureKey {
                    font: self.font_for_text(text),
                    text: text.into(),
                    size: size.to_bits(),
                    spacing: (size * 0.02).max(1.0).to_bits(),
                },
                fill,
                stroke: style.color,
                width: style.width,
            };
            if let Some(cached) = self.strokes.get(&key) {
                let texture = Self::texture_view(&cached.texture);
                let position = anchored(Point { x, y }, cached.measured, style.anchor);
                let padding = cached.padding;
                self.stroke_order.retain(|value| value != &key);
                self.stroke_order.push_back(key);
                return self.gfx.texture_pro(
                    texture,
                    Rect::new(
                        0.0,
                        0.0,
                        f64::from(texture.width),
                        f64::from(texture.height),
                    ),
                    Rect::new(
                        position.x - padding,
                        position.y - padding,
                        f64::from(texture.width),
                        f64::from(texture.height),
                    ),
                    Point::default(),
                    0.0,
                    [255, 255, 255],
                );
            }
            if !self.pending_strokes.contains(&key) {
                self.pending_order.push_back(key.clone());
                self.pending_strokes.insert(key);
            }
        }
        if style.width > 0 {
            if self.text_policy.raw_strokes {
                return self.draw_raw_stroke(text, Point::new(x, y), size, fill, style);
            }
            let width = f64::from(style.width);
            for (dx, dy) in [
                (-width, 0.0),
                (width, 0.0),
                (0.0, -width),
                (0.0, width),
                (-width, -width),
                (width, -width),
                (-width, width),
                (width, width),
            ] {
                self.draw_text(text, x + dx, y + dy, size, style.color, style.anchor)?;
            }
        }
        self.draw_text(text, x, y, size, fill, style.anchor)
    }
    pub(super) fn flush_pending_strokes(&mut self) -> Result<()> {
        if self.pending_order.is_empty() {
            return Ok(());
        }
        if !self.text_policy.strokes {
            self.pending_strokes.clear();
            self.pending_order.clear();
            return Ok(());
        }
        let start = self.profile_start();
        while let Some(key) = self.pending_order.pop_front() {
            self.pending_strokes.remove(&key);
            if self.strokes.contains_key(&key) {
                continue;
            }
            let size = f64::from_bits(key.measure.size);
            let spacing = f64::from_bits(key.measure.spacing);
            let measured = self.measure_text(&key.measure.text, size, Some(spacing))?;
            let padding = f64::from(key.width.max(0))
                + STROKED_TEXT_TEXTURE_PADDING_PX.to_f64().unwrap_or(2.0);
            let width = (measured.0 + padding * 2.0)
                .ceil()
                .max(1.0)
                .to_i32()
                .ok_or(super::Error::Contract("text texture width invalid"))?;
            let height = (measured.1 + padding * 2.0)
                .ceil()
                .max(1.0)
                .to_i32()
                .ok_or(super::Error::Contract("text texture height invalid"))?;
            let Ok(mut image) = RawImage::blank((width, height), 0) else {
                continue;
            };
            let stroke = f64::from(key.width);
            for (dx, dy) in [
                (-stroke, 0.0),
                (stroke, 0.0),
                (0.0, -stroke),
                (0.0, stroke),
                (-stroke, -stroke),
                (stroke, -stroke),
                (-stroke, stroke),
                (stroke, stroke),
            ] {
                self.gfx.backend()?.raw_image_text(
                    &mut image,
                    RawTextDraw {
                        font: key.measure.font,
                        text: &key.measure.text,
                        position: point(Point::new(padding + dx, padding + dy))?,
                        size: scalar(size)?,
                        spacing: scalar(spacing)?,
                        color: color(key.stroke),
                    },
                )?;
            }
            self.gfx.backend()?.raw_image_text(
                &mut image,
                RawTextDraw {
                    font: key.measure.font,
                    text: &key.measure.text,
                    position: point(Point::new(padding, padding))?,
                    size: scalar(size)?,
                    spacing: scalar(spacing)?,
                    color: color(key.fill),
                },
            )?;
            let Ok(pixels) = image.rgba() else {
                continue;
            };
            let texture = self.gfx.backend()?.dynamic_filtered_image(
                PixelBuffer {
                    dimensions: (width, height),
                    rgba: &pixels,
                },
                TextureFilter::Bilinear,
            );
            let Ok(texture) = texture else {
                continue;
            };
            self.strokes.insert(
                key.clone(),
                CachedText {
                    texture,
                    measured,
                    padding,
                },
            );
            self.stroke_order.push_back(key);
            while self.strokes.len() > STROKED_TEXT_TEXTURE_CACHE_LIMIT {
                if let Some(old) = self.stroke_order.pop_front() {
                    self.strokes.remove(&old);
                }
            }
        }
        self.profile_add("stroked_text_texture_cache.flush", start);
        Ok(())
    }
    fn draw_raw_stroke(
        &mut self,
        text: &str,
        position: Point,
        size: f64,
        fill: [u8; 4],
        style: StrokeStyle,
    ) -> Result<()> {
        let measured = self.measure_text(text, size, None)?;
        let font = self.font_for_text(text);
        let spacing = scalar((size * 0.02).max(1.0))?;
        let render_size = scalar(size)?;
        let width = f64::from(style.width);
        let offsets = [
            (-width, 0.0),
            (width, 0.0),
            (0.0, -width),
            (0.0, width),
            (-width, -width),
            (width, -width),
            (-width, width),
            (width, width),
            (0.0, 0.0),
        ];
        let initial = RawTextPaint {
            font,
            position: RawPoint::default(),
            size: render_size,
            spacing,
            color: color(style.color),
        };
        let mut paints = [initial; 9];
        for (index, (paint, (dx, dy))) in paints.iter_mut().zip(offsets).enumerate() {
            let positioned = point(anchored(
                Point::new(position.x + dx, position.y + dy),
                measured,
                style.anchor,
            ))?;
            paint.position = RawPoint {
                x: positioned.x,
                y: positioned.y,
            };
            if index == 8 {
                paint.color = color(fill);
            }
        }
        Ok(self.gfx.backend()?.raw_text_batch(text, &paints)?)
    }
    /// # Errors
    /// Returns font, coordinate, or native drawing errors.
    pub fn ellipsize_text(&mut self, text: &str, size: f64, width: f64) -> Result<String> {
        if self.measure_text(text, size, None)?.0 <= width {
            return Ok(text.into());
        }
        let chars: Vec<char> = text.chars().collect();
        let mut low = 0;
        let mut high = chars.len();
        while low < high {
            let mid = (low + high).div_ceil(2);
            let candidate = chars[..mid].iter().collect::<String>() + "...";
            if self.measure_text(&candidate, size, None)?.0 <= width {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        Ok(chars[..low].iter().collect::<String>() + "...")
    }
    /// # Errors
    /// Returns font, coordinate, or native drawing errors.
    pub fn rounded_rect(
        &mut self,
        rect: Rect,
        radius: f64,
        fill: impl IntoColor,
        outline: Option<[u8; 4]>,
        outline_width: f64,
    ) -> Result<()> {
        let roundness = (radius / rect.width.min(rect.height).max(1.0)).clamp(0.0, 1.0);
        self.gfx.rounded(rect, roundness, 12, fill)?;
        if let Some(outline) = outline.filter(|_| outline_width > 0.0) {
            self.gfx
                .rounded_lines(rect, roundness, 12, outline_width, outline)?;
        }
        Ok(())
    }
    /// # Errors
    /// Returns font, coordinate, or native texture drawing errors.
    pub fn draw_world_label_text(
        &mut self,
        text: &str,
        x: f64,
        y: f64,
        size: f64,
        tint: impl IntoColor,
        anchor: Anchor,
    ) -> Result<()> {
        let fill = tint.rgba();
        if !self.text_policy.world_labels {
            return self.draw_text(text, x, y, size, fill, anchor);
        }
        let grid = super::tokens::WORLD_LABEL_TEXTURE_SIZE_GRID;
        let render_size = ((size / grid).round_ties_even() * grid).max(1.0);
        let spacing = (render_size * 0.02).max(1.0);
        let font = self.font_for_text(text);
        let key = WorldKey {
            measure: MeasureKey {
                font,
                text: text.into(),
                size: render_size.to_bits(),
                spacing: spacing.to_bits(),
            },
            fill,
        };
        if !self.world_text.contains_key(&key) {
            let stage = self.profile_start();
            let measured = self.measure_text(text, render_size, Some(spacing))?;
            let padding = super::tokens::WORLD_LABEL_TEXTURE_PADDING_PX
                .to_f64()
                .ok_or(super::Error::Contract("world text padding invalid"))?;
            let width = (measured.0 + padding * 2.0)
                .ceil()
                .max(1.0)
                .to_i32()
                .ok_or(super::Error::Contract("world text width invalid"))?;
            let height = (measured.1 + padding * 2.0)
                .ceil()
                .max(1.0)
                .to_i32()
                .ok_or(super::Error::Contract("world text height invalid"))?;
            let created = (|| -> Result<DynamicTexture> {
                let mut image = RawImage::blank((width, height), 0)?;
                self.gfx.backend()?.raw_image_text(
                    &mut image,
                    RawTextDraw {
                        font,
                        text,
                        position: point(Point::new(padding, padding))?,
                        size: scalar(render_size)?,
                        spacing: scalar(spacing)?,
                        color: color(fill),
                    },
                )?;
                let pixels = image.rgba()?;
                Ok(self.gfx.backend()?.dynamic_pixels(width, height, &pixels)?)
            })();
            let Ok(texture) = created else {
                return self.draw_text(text, x, y, size, fill, anchor);
            };
            self.world_text.insert(
                key.clone(),
                CachedText {
                    texture,
                    measured,
                    padding,
                },
            );
            self.world_text_order.push_back(key.clone());
            while self.world_text.len() > super::tokens::WORLD_LABEL_TEXTURE_CACHE_LIMIT {
                if let Some(old) = self.world_text_order.pop_front() {
                    self.world_text.remove(&old);
                }
            }
            self.profile_add("world_label_texture_cache.miss", stage);
        }
        let Some(cached) = self.world_text.get(&key) else {
            return self.draw_text(text, x, y, size, fill, anchor);
        };
        let texture = Self::texture_view(&cached.texture);
        let position = anchored(Point::new(x, y), cached.measured, anchor);
        let padding = cached.padding;
        self.world_text_order.retain(|value| value != &key);
        self.world_text_order.push_back(key);
        self.gfx.texture_pro(
            texture,
            Rect::new(
                0.0,
                0.0,
                f64::from(texture.width),
                f64::from(texture.height),
            ),
            Rect::new(
                position.x - padding,
                position.y - padding,
                f64::from(texture.width),
                f64::from(texture.height),
            ),
            Point::default(),
            0.0,
            [255, 255, 255],
        )
    }
}
fn anchored(mut position: Point, measured: (f64, f64), anchor: Anchor) -> Point {
    position.y -= measured.1 * 0.5;
    match anchor {
        Anchor::Left => (),
        Anchor::Center => position.x -= measured.0 * 0.5,
        Anchor::Right => position.x -= measured.0,
    }
    position
}
