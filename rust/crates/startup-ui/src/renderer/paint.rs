use super::*;
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
    fn clear(&mut self, color: u32) -> Result<(), Error> {
        self.surface.pin_mut().clear(color);
        Ok(())
    }
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
        if let Some(rect) = rect {
            for value in [rect.x, rect.y, rect.width, rect.height] {
                crate::number::coordinate(value)?;
            }
        }
        self.surface
            .pin_mut()
            .scissor(convert(rect.unwrap_or_default()), rect.is_some());
        Ok(())
    }
}
pub(super) fn convert(rect: Rect) -> ffi::Rect {
    ffi::Rect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}
