use super::layout::{self, Frame, Measure, Paint, Screen};
use crate::{
    draw::Draw,
    geometry::{Point, Rect},
    renderer::{RawTextDraw, Renderer},
    Error,
};
use std::{collections::BTreeSet, path::Path};
struct Font<'a> {
    renderer: &'a Renderer,
    font: u32,
}
impl Measure for Font<'_> {
    fn width(&self, text: &str, size: f64) -> Result<f64, Error> {
        Ok(f64::from(
            self.renderer
                .raw_font_measure(self.font, text, size as f32, 0.0)?
                .x,
        ))
    }
}
pub struct View {
    pub renderer: Renderer,
    native_repair: bool,
    font: u32,
    dimensions: (i32, i32),
    reason: String,
    pub pages: Vec<Vec<String>>,
    pub page: usize,
}
impl View {
    pub fn new(
        root: &Path,
        dimensions: (i32, i32),
        reason: &str,
        error: &str,
        preview: bool,
        native_repair: bool,
    ) -> Result<Self, Error> {
        let mut renderer = if preview {
            Renderer::raw_window(dimensions, "Startup recovery", 128, 20)?
        } else {
            Renderer::raw_window_inheriting_flags(dimensions, "Startup recovery", 20)?
        };
        renderer.set_exit_key(0);
        let scale = (f64::from(dimensions.0) / 536.0).min(f64::from(dimensions.1) / 240.0);
        let mut chars = BTreeSet::from_iter(32..127);
        let labels = layout::labels(native_repair);
        for text in labels.iter().flat_map(|(_, ko, en)| [*ko, *en]).chain([
            layout::automatic_label(native_repair),
            layout::BUTTON_KO,
            layout::repair_label(native_repair),
            layout::ERROR_KO,
            reason,
            error,
        ]) {
            chars.extend(
                text.chars()
                    .filter(|c| u32::from(*c) >= 32)
                    .map(|c| u32::from(c) as i32),
            );
        }
        let points = chars.into_iter().collect::<Vec<_>>();
        let font = renderer.raw_font(
            &root.join("openpilot/selfdrive/assets/fonts/Pretendard-Medium.ttf"),
            (28.0 * scale).round_ties_even() as i32,
            &points,
        )?;
        let pages = layout::pages(
            error,
            scale,
            &Font {
                renderer: &renderer,
                font,
            },
        )?;
        Ok(Self {
            renderer,
            native_repair,
            font,
            dimensions,
            reason: reason.into(),
            pages,
            page: 0,
        })
    }
    pub fn next_page(&mut self) {
        self.page = (self.page + 1) % self.pages.len();
    }
    pub fn frame(&self, status: &str, detail: &str, address: &str) -> Result<Frame, Error> {
        layout::frame(
            Screen {
                native_repair: self.native_repair,
                dimensions: self.dimensions,
                status,
                detail,
                reason: &self.reason,
                address,
                pages: &self.pages,
                page: self.page,
            },
            &Font {
                renderer: &self.renderer,
                font: self.font,
            },
        )
    }
    pub fn paint(&mut self, frame: &Frame) -> Result<(), Error> {
        for command in &frame.commands {
            match command {
                Paint::Clear(color) => self.renderer.clear(u32::from_le_bytes(*color))?,
                Paint::Text {
                    text,
                    position,
                    size,
                    color,
                } => self.renderer.raw_text(RawTextDraw {
                    font: self.font,
                    text,
                    position: Point {
                        x: position[0],
                        y: position[1],
                    },
                    size: *size,
                    spacing: 0.0,
                    color: u32::from_le_bytes(*color),
                })?,
                Paint::Rectangle {
                    rect,
                    roundness,
                    segments,
                    color,
                } => self.renderer.rounded_segments(
                    Rect {
                        x: rect[0],
                        y: rect[1],
                        width: rect[2],
                        height: rect[3],
                    },
                    *roundness,
                    *segments,
                    u32::from_le_bytes(*color),
                    false,
                ),
            }
        }
        Ok(())
    }
    pub fn preview(
        &mut self,
        status: &str,
        detail: &str,
        address: &str,
        path: &Path,
    ) -> Result<[[f32; 4]; 3], Error> {
        let frame = self.frame(status, detail, address)?;
        self.renderer.ensure_render_target(self.dimensions)?;
        self.renderer.begin_capture()?;
        let painted = self.paint(&frame);
        self.renderer.end_capture();
        painted?;
        self.renderer.screenshot(path)?;
        Ok(frame.areas)
    }
}
